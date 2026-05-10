//! Daemon core: owns the audio session and library cache.
//!
//! This is the type that will be lifted into the standalone `ferrosonicd`
//! binary in phase 5. Phase 2 keeps it in-process — `App` holds an
//! `Arc<DaemonCore>`, methods on `App` delegate here. Lock structure:
//!
//! - `state`: `SharedState` (`Arc<RwLock<AppState>>`) — phase 2 sharing
//!   so the existing TUI render path keeps working unchanged. DaemonCore
//!   only mutates the `state.daemon: DaemonState` half (queue, now_playing,
//!   library, config); never `state.client: ClientState`. Phase 5 swaps
//!   this for `Arc<RwLock<DaemonState>>` when DaemonCore moves into its
//!   own process. RwLock so MPRIS can read concurrently with playback
//!   updates without contention.
//! - `mpv`: `Mutex<MpvController>` — mpv has only one IPC socket and
//!   `send_command` requires &mut, so all access is serialised.
//! - `pipewire`: `Mutex<PipeWireController>` — `set_rate` needs &mut and
//!   shells out to `pw-metadata`; serialise.
//! - `subsonic`: `RwLock<Option<SubsonicClient>>` — RwLock to allow many
//!   concurrent reads (the underlying reqwest client is internally
//!   thread-safe); write is rare (only on UpdateServerConfig).
//! - `event_tx`: `broadcast::Sender<DaemonEvent>` — fan-out to subscribed
//!   clients. Capacity 256; slow consumers see RecvError::Lagged and must
//!   re-subscribe to resnapshot.

use std::sync::Arc;

use tokio::sync::{broadcast, Mutex, RwLock};
use tracing::{debug, error, info, warn};

use crate::app::state::SharedState;
use crate::audio::mpv::MpvController;
use crate::audio::pipewire::PipeWireController;
use crate::config::Config;
use crate::daemon::persistence::QueueSnapshot;
use crate::daemon::state::DaemonState;
use crate::error::Error;
use crate::ipc::protocol::DaemonEvent;
use crate::subsonic::SubsonicClient;

/// Capacity of the broadcast channel for daemon events. Slow consumers
/// will see `Lagged` once they fall behind by this many events.
const EVENT_CHANNEL_CAPACITY: usize = 256;

/// Centralised audio + library state. Single instance per daemon process.
/// Cheap to clone the `Arc<DaemonCore>` to share across tasks.
///
/// Phase 2 holds a `SharedState` (`Arc<RwLock<AppState>>`) so the existing
/// TUI render path keeps working unchanged — DaemonCore's methods only
/// touch `state.daemon.X` (audio session + library cache + config), never
/// `state.client.X`. Phase 5 splits this into a separate process where
/// DaemonCore owns just `Arc<RwLock<DaemonState>>`.
pub struct DaemonCore {
    /// Shared application state. DaemonCore methods only mutate the
    /// `daemon: DaemonState` half of this — never `client: ClientState`.
    pub state: SharedState,
    /// MPV process + IPC socket controller.
    pub mpv: Mutex<MpvController>,
    /// PipeWire sample-rate controller.
    pub pipewire: Mutex<PipeWireController>,
    /// Subsonic API client (None when config is unconfigured).
    pub subsonic: RwLock<Option<SubsonicClient>>,
    /// Fan-out channel for state-change events. Clients subscribe via
    /// `event_tx.subscribe()` and consume the resulting `Receiver`.
    pub event_tx: broadcast::Sender<DaemonEvent>,
    /// Trailing-edge debounce signal for the queue-persistence task.
    /// `try_send(())` on every queue change; the task drains it, sleeps
    /// briefly, then writes the latest queue to disk.
    queue_save_tx: tokio::sync::mpsc::Sender<()>,
}

impl DaemonCore {
    /// Build a new core wrapping the given `SharedState`. Creates an
    /// `MpvController` (does not start mpv yet — call `start_mpv()` when
    /// ready), a `PipeWireController`, and a `SubsonicClient` if the
    /// config is configured.
    pub fn new(state: SharedState, config: &Config) -> Arc<Self> {
        let subsonic = if config.is_configured() {
            match SubsonicClient::new(&config.base_url, &config.username, &config.password) {
                Ok(client) => Some(client),
                Err(e) => {
                    warn!("Failed to create Subsonic client: {}", e);
                    None
                }
            }
        } else {
            None
        };

        let (event_tx, _) = broadcast::channel(EVENT_CHANNEL_CAPACITY);
        let (queue_save_tx, queue_save_rx) = tokio::sync::mpsc::channel::<()>(1);

        let core = Arc::new(Self {
            state,
            mpv: Mutex::new(MpvController::new()),
            pipewire: Mutex::new(PipeWireController::new()),
            subsonic: RwLock::new(subsonic),
            event_tx,
            queue_save_tx,
        });

        core.clone().spawn_queue_persistence(queue_save_rx);
        core.clone().restore_queue_blocking();
        core
    }

    fn restore_queue_blocking(self: Arc<Self>) {
        if let Some(snap) = QueueSnapshot::load() {
            let count = snap.queue.len();
            let position = snap.position;
            let st = self.state.clone();
            tokio::spawn(async move {
                let mut s = st.write().await;
                s.daemon.queue = snap.queue;
                s.daemon.queue_position = snap.position;
                info!("Restored {} queue items (position={:?})", count, position);
            });
        }
    }

    fn spawn_queue_persistence(
        self: Arc<Self>,
        mut rx: tokio::sync::mpsc::Receiver<()>,
    ) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            while rx.recv().await.is_some() {
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                while rx.try_recv().is_ok() {} // coalesce burst
                let snap = {
                    let s = self.state.read().await;
                    QueueSnapshot {
                        queue: s.daemon.queue.clone(),
                        position: s.daemon.queue_position,
                    }
                };
                if let Err(e) = snap.save() {
                    warn!("Queue persistence write failed: {}", e);
                }
            }
        })
    }

    /// Start the mpv subprocess. Idempotent — no-ops if already running.
    pub async fn start_mpv(&self) -> Result<(), Error> {
        let mut mpv = self.mpv.lock().await;
        mpv.start().map_err(Into::into)
    }

    /// Best-effort shutdown of mpv. Used at TUI exit (phase 2) and at
    /// daemon shutdown (phase 7).
    pub async fn quit_mpv(&self) {
        let mut mpv = self.mpv.lock().await;
        let _ = mpv.quit();
    }

    /// Spawn the playback-info polling task. Runs `update_playback_info`
    /// every 500ms on the tokio runtime. The returned `JoinHandle` is
    /// detached by `App::run` (we don't await it; cancellation happens
    /// at process exit). Phase 5 keeps this exact loop in the daemon
    /// process — it is fully self-contained.
    pub fn spawn_polling_task(self: &Arc<Self>) -> tokio::task::JoinHandle<()> {
        let core = self.clone();
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(std::time::Duration::from_millis(500));
            tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                tick.tick().await;
                core.update_playback_info().await;
            }
        })
    }

    /// Subscribe to daemon events. Returns a `broadcast::Receiver` that
    /// will receive every `DaemonEvent` broadcast after the call.
    /// Used by the upcoming `DaemonClient` (phase 2.3) and the socket
    /// server (phase 4); not yet wired in phase 2.2.
    #[allow(dead_code)]
    pub fn subscribe(&self) -> broadcast::Receiver<DaemonEvent> {
        self.event_tx.subscribe()
    }

    /// Internal: emit an event. Errors are logged and ignored — a slow or
    /// disconnected subscriber shouldn't block the daemon.
    fn emit(&self, event: DaemonEvent) {
        // `send` returns Err only if there are no subscribers, which is
        // fine — events are best-effort fan-out.
        let _ = self.event_tx.send(event);
    }

    /// Snapshot the current `NowPlaying` and emit it. Convenience for
    /// the many call sites that mutated `state.daemon.now_playing` and
    /// then dropped the lock — they now don't have to re-clone it.
    async fn emit_now_playing(&self) {
        let np = {
            let state = self.state.read().await;
            state.daemon.now_playing.clone()
        };
        self.emit(DaemonEvent::NowPlayingChanged(np));
    }

    /// Snapshot the current queue + position and emit. Same convenience
    /// rationale as `emit_now_playing`.
    async fn emit_queue(&self) {
        let (queue, position) = {
            let state = self.state.read().await;
            (state.daemon.queue.clone(), state.daemon.queue_position)
        };
        let _ = self.queue_save_tx.try_send(());
        self.emit(DaemonEvent::QueueChanged { queue, position });
    }

    /// Build a snapshot of the daemon state for a connecting client.
    /// The Subsonic password is scrubbed before sending — the TUI never
    /// makes server requests directly, only the daemon does, so it
    /// doesn't need the credential.
    pub async fn snapshot(&self) -> DaemonState {
        let mut snap = {
            let state = self.state.read().await;
            state.daemon.clone()
        };
        snap.config.password.clear();
        snap.config.password_file = None;
        snap
    }

    async fn emit_config_changed(&self) {
        let mut cfg = {
            let state = self.state.read().await;
            state.daemon.config.clone()
        };
        cfg.password.clear();
        cfg.password_file = None;
        self.emit(DaemonEvent::ConfigChanged(cfg));
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Library fetches (was App's repo.rs).
// All methods read self.subsonic, write to self.state.daemon.library, emit
// LibraryChanged events, push notifications on error.
// ─────────────────────────────────────────────────────────────────────────

impl DaemonCore {
    pub async fn refresh_starred(self: &Arc<Self>) {
        let Some(client) = self.subsonic.read().await.clone() else { return; };
        match client.get_starred_songs().await {
            Ok(songs) => {
                let mut state = self.state.write().await;
                state.daemon.library.starred_songs = songs.clone();
                drop(state);
                self.emit(DaemonEvent::StarredChanged(songs));
            }
            Err(e) => {
                error!("Failed to load starred songs: {}", e);
                self.emit(DaemonEvent::Notification {
                    message: format!("Failed to load starred songs: {}", e),
                    is_error: true,
                });
            }
        }
    }

    pub async fn refresh_random(self: &Arc<Self>) {
        let Some(client) = self.subsonic.read().await.clone() else { return; };
        match client.get_random_songs().await {
            Ok(songs) => {
                let mut state = self.state.write().await;
                state.daemon.library.random_songs = songs.clone();
                drop(state);
                self.emit(DaemonEvent::RandomChanged(songs));
            }
            Err(e) => {
                error!("Failed to load random songs: {}", e);
                self.emit(DaemonEvent::Notification {
                    message: format!("Failed to load random songs: {}", e),
                    is_error: true,
                });
            }
        }
    }

    pub async fn refresh_artists(self: &Arc<Self>) {
        let Some(client) = self.subsonic.read().await.clone() else { return; };
        match client.get_artists().await {
            Ok(artists) => {
                let mut state = self.state.write().await;
                let count = artists.len();
                state.daemon.library.artists = artists.clone();
                drop(state);
                info!("Loaded {} artists", count);
                self.emit(DaemonEvent::ArtistsChanged(artists));
            }
            Err(e) => {
                error!("Failed to load artists: {}", e);
                self.emit(DaemonEvent::Notification {
                    message: format!("Failed to load artists: {}", e),
                    is_error: true,
                });
            }
        }
    }

    pub async fn refresh_playlists(self: &Arc<Self>) {
        let Some(client) = self.subsonic.read().await.clone() else { return; };
        match client.get_playlists().await {
            Ok(playlists) => {
                let mut state = self.state.write().await;
                let count = playlists.len();
                state.daemon.library.playlists = playlists.clone();
                drop(state);
                info!("Loaded {} playlists", count);
                self.emit(DaemonEvent::PlaylistsChanged(playlists));
            }
            Err(e) => {
                error!("Failed to load playlists: {}", e);
                // Don't show error for playlists if artists loaded
            }
        }
    }

    pub async fn toggle_star_song(self: &Arc<Self>, song_id: &str) -> Result<bool, Error> {
        let currently_starred = {
            let state = self.state.read().await;
            song_is_starred(&state.daemon, song_id)
        };

        let Some(client) = self.subsonic.read().await.clone() else {
            return Err(Error::Subsonic(crate::error::SubsonicError::Api {
                code: 0,
                message: "Subsonic client not configured".to_string(),
            }));
        };

        if currently_starred {
            client.unstar_song(song_id).await.map_err(Error::Subsonic)?;
        } else {
            client.star_song(song_id).await.map_err(Error::Subsonic)?;
        }

        let new_starred = !currently_starred;
        {
            let mut state = self.state.write().await;
            apply_star_to_cached(&mut state.daemon, song_id, new_starred);
        }
        self.emit(DaemonEvent::SongStarChanged {
            id: song_id.to_string(),
            starred: new_starred,
        });
        self.refresh_starred().await;
        Ok(new_starred)
    }

    pub async fn load_artist(self: &Arc<Self>, artist_id: &str) {
        let Some(client) = self.subsonic.read().await.clone() else { return; };
        match client.get_artist(artist_id).await {
            Ok((_artist, albums)) => {
                let mut state = self.state.write().await;
                let count = albums.len();
                crate::daemon::library::cache_insert(
                    &mut state.daemon.library.albums_cache,
                    artist_id.to_string(),
                    albums.clone(),
                    crate::daemon::library::ALBUMS_CACHE_CAP,
                );
                drop(state);
                info!("Loaded {} albums for {}", count, artist_id);
                self.emit(DaemonEvent::AlbumsChanged {
                    artist_id: artist_id.to_string(),
                    albums,
                });
            }
            Err(e) => {
                error!("Failed to load albums: {}", e);
                self.emit(DaemonEvent::Notification {
                    message: format!("Failed to load albums: {}", e),
                    is_error: true,
                });
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Playback (was App's playback.rs).
// All methods read/write self.state, drive self.mpv via locked access,
// drive self.pipewire for sample-rate switching, emit NowPlayingChanged /
// QueueChanged events.
// ─────────────────────────────────────────────────────────────────────────

impl DaemonCore {
    /// Toggle play/pause. No-op if neither playing nor paused.
    pub async fn toggle_pause(self: &Arc<Self>) -> Result<(), Error> {
        use crate::app::state::PlaybackState;
        let snapshot = {
            let state = self.state.read().await;
            (state.daemon.now_playing.state,)
        };
        let (playback_state,) = snapshot;
        if playback_state != PlaybackState::Playing && playback_state != PlaybackState::Paused {
            return Ok(());
        }

        let mut mpv = self.mpv.lock().await;
        match mpv.toggle_pause() {
            Ok(now_paused) => {
                drop(mpv);
                let mut state = self.state.write().await;
                state.daemon.now_playing.state = if now_paused {
                    PlaybackState::Paused
                } else {
                    PlaybackState::Playing
                };
                debug!("toggle_pause: now {:?}", state.daemon.now_playing.state);
                drop(state);
                self.emit_now_playing().await;
            }
            Err(e) => {
                error!("Failed to toggle pause: {}", e);
            }
        }
        Ok(())
    }

    /// Pause playback. No-op if not playing.
    pub async fn pause_playback(self: &Arc<Self>) -> Result<(), Error> {
        use crate::app::state::PlaybackState;
        {
            let state = self.state.read().await;
            if state.daemon.now_playing.state != PlaybackState::Playing {
                return Ok(());
            }
        }
        let mut mpv = self.mpv.lock().await;
        match mpv.pause() {
            Ok(()) => {
                drop(mpv);
                let mut state = self.state.write().await;
                state.daemon.now_playing.state = PlaybackState::Paused;
                drop(state);
                self.emit_now_playing().await;
            }
            Err(e) => error!("Failed to pause: {}", e),
        }
        Ok(())
    }

    /// Resume playback. No-op if not paused.
    pub async fn resume_playback(self: &Arc<Self>) -> Result<(), Error> {
        use crate::app::state::PlaybackState;
        {
            let state = self.state.read().await;
            if state.daemon.now_playing.state != PlaybackState::Paused {
                return Ok(());
            }
        }
        let mut mpv = self.mpv.lock().await;
        match mpv.resume() {
            Ok(()) => {
                drop(mpv);
                let mut state = self.state.write().await;
                state.daemon.now_playing.state = PlaybackState::Playing;
                drop(state);
                self.emit_now_playing().await;
            }
            Err(e) => error!("Failed to resume: {}", e),
        }
        Ok(())
    }

    /// Skip to the next track in the queue. If at end, stops playback.
    pub async fn next_track(self: &Arc<Self>) -> Result<(), Error> {
        use crate::app::state::PlaybackState;
        let (queue_len, current_pos) = {
            let state = self.state.read().await;
            (state.daemon.queue.len(), state.daemon.queue_position)
        };
        if queue_len == 0 {
            return Ok(());
        }
        let next_pos = match current_pos {
            Some(pos) if pos + 1 < queue_len => pos + 1,
            _ => {
                info!("Reached end of queue");
                let mut mpv = self.mpv.lock().await;
                let _ = mpv.stop();
                drop(mpv);
                let mut state = self.state.write().await;
                state.daemon.now_playing.state = PlaybackState::Stopped;
                state.daemon.now_playing.position = 0.0;
                drop(state);
                self.emit_now_playing().await;
                return Ok(());
            }
        };
        self.play_queue_position(next_pos).await
    }

    /// Previous track in the queue, with the standard "restart current
    /// track if more than 3s elapsed" behaviour.
    pub async fn prev_track(self: &Arc<Self>) -> Result<(), Error> {
        let (queue_len, current_pos, position) = {
            let state = self.state.read().await;
            (
                state.daemon.queue.len(),
                state.daemon.queue_position,
                state.daemon.now_playing.position,
            )
        };
        if queue_len == 0 {
            return Ok(());
        }
        if position < 3.0 {
            if let Some(pos) = current_pos {
                if pos > 0 {
                    return self.play_queue_position(pos - 1).await;
                }
            }
            // At track 0 with <3s elapsed — restart from 0
            let mut mpv = self.mpv.lock().await;
            if let Err(e) = mpv.seek(0.0) {
                error!("Failed to restart track: {}", e);
            } else {
                drop(mpv);
                let mut state = self.state.write().await;
                state.daemon.now_playing.position = 0.0;
            }
            return Ok(());
        }
        // Restart current track from 0
        let mut mpv = self.mpv.lock().await;
        if let Err(e) = mpv.seek(0.0) {
            error!("Failed to restart track: {}", e);
        } else {
            drop(mpv);
            let mut state = self.state.write().await;
            state.daemon.now_playing.position = 0.0;
        }
        Ok(())
    }

    /// Load and play the song at queue position `pos`. Replaces mpv's
    /// playlist. Updates now_playing, queue_position. Preloads the next
    /// track for gapless playback.
    pub async fn play_queue_position(self: &Arc<Self>, pos: usize) -> Result<(), Error> {
        use crate::app::state::PlaybackState;
        let song = {
            let state = self.state.read().await;
            match state.daemon.queue.get(pos) {
                Some(s) => s.clone(),
                None => return Ok(()),
            }
        };

        let stream_url = {
            let Some(client) = self.subsonic.read().await.clone() else { return Ok(()); };
            match client.get_stream_url(&song.id) {
                Ok(url) => url,
                Err(e) => {
                    error!("Failed to get stream URL: {}", e);
                    self.emit(DaemonEvent::Notification {
                        message: format!("Failed to get stream URL: {}", e),
                        is_error: true,
                    });
                    return Ok(());
                }
            }
        };

        {
            let mut state = self.state.write().await;
            state.daemon.queue_position = Some(pos);
            state.daemon.now_playing.song = Some(song.clone());
            state.daemon.now_playing.state = PlaybackState::Playing;
            state.daemon.now_playing.position = 0.0;
            state.daemon.now_playing.duration = song.duration.unwrap_or(0) as f64;
            state.daemon.now_playing.sample_rate = None;
            state.daemon.now_playing.bit_depth = None;
            state.daemon.now_playing.format = None;
            state.daemon.now_playing.channels = None;
        }

        info!("Playing: {} (queue pos {})", song.title, pos);
        {
            let mut mpv = self.mpv.lock().await;
            if mpv.is_paused().unwrap_or(false) {
                let _ = mpv.resume();
            }
            if let Err(e) = mpv.loadfile(&stream_url) {
                error!("Failed to play: {}", e);
                drop(mpv);
                self.emit(DaemonEvent::Notification {
                    message: format!("MPV error: {}", e),
                    is_error: true,
                });
                return Ok(());
            }
        }

        self.preload_next_track(pos).await;
        self.emit_now_playing().await;
        self.emit_queue().await;
        Ok(())
    }

    /// Pre-load the next queue track into mpv's playlist for gapless playback.
    pub async fn preload_next_track(self: &Arc<Self>, current_pos: usize) {
        let next_song = {
            let state = self.state.read().await;
            let next_pos = current_pos + 1;
            if next_pos >= state.daemon.queue.len() {
                return;
            }
            match state.daemon.queue.get(next_pos) {
                Some(s) => s.clone(),
                None => return,
            }
        };

        let url = {
            let Some(client) = self.subsonic.read().await.clone() else { return; };
            match client.get_stream_url(&next_song.id) {
                Ok(u) => u,
                Err(_) => return,
            }
        };

        debug!("Pre-loading next track for gapless: {}", next_song.title);
        let mut mpv = self.mpv.lock().await;
        if let Err(e) = mpv.loadfile_append(&url) {
            debug!("Failed to pre-load next track: {}", e);
        } else if let Ok(count) = mpv.get_playlist_count() {
            if count < 2 {
                warn!("Preload may have failed: playlist count is {} (expected 2)", count);
            } else {
                debug!("Preload confirmed: playlist count is {}", count);
            }
        }
    }

    /// Stop playback and clear the queue.
    pub async fn stop_playback(self: &Arc<Self>) -> Result<(), Error> {
        use crate::app::state::PlaybackState;
        {
            let mut mpv = self.mpv.lock().await;
            if let Err(e) = mpv.stop() {
                error!("Failed to stop: {}", e);
            }
        }
        let mut state = self.state.write().await;
        state.daemon.now_playing.state = PlaybackState::Stopped;
        state.daemon.now_playing.song = None;
        state.daemon.now_playing.position = 0.0;
        state.daemon.now_playing.duration = 0.0;
        state.daemon.now_playing.sample_rate = None;
        state.daemon.now_playing.bit_depth = None;
        state.daemon.now_playing.format = None;
        state.daemon.now_playing.channels = None;
        state.daemon.queue.clear();
        state.daemon.queue_position = None;
        drop(state);
        self.emit_now_playing().await;
        self.emit_queue().await;
        Ok(())
    }

    /// Stop mpv and clear now-playing, but leave the queue intact. Used
    /// when removing the currently-playing entry from a queue that
    /// otherwise still has songs.
    pub async fn halt_keep_queue(self: &Arc<Self>) {
        use crate::app::state::PlaybackState;
        {
            let mut mpv = self.mpv.lock().await;
            if let Err(e) = mpv.stop() {
                error!("Failed to stop: {}", e);
            }
        }
        {
            let mut state = self.state.write().await;
            state.daemon.now_playing.state = PlaybackState::Stopped;
            state.daemon.now_playing.song = None;
            state.daemon.now_playing.position = 0.0;
            state.daemon.now_playing.duration = 0.0;
            state.daemon.now_playing.sample_rate = None;
            state.daemon.now_playing.bit_depth = None;
            state.daemon.now_playing.format = None;
            state.daemon.now_playing.channels = None;
        }
        self.emit_now_playing().await;
    }

    /// Direct mpv seek by absolute position in seconds. Updates now_playing
    /// position on success.
    pub async fn seek(self: &Arc<Self>, pos: f64) -> Result<(), Error> {
        let mut mpv = self.mpv.lock().await;
        if let Err(e) = mpv.seek(pos) {
            warn!("Seek failed: {}", e);
            return Ok(());
        }
        drop(mpv);
        let mut state = self.state.write().await;
        state.daemon.now_playing.position = pos;
        Ok(())
    }

    /// Direct mpv seek by relative offset.
    pub async fn seek_relative(self: &Arc<Self>, offset: f64) -> Result<(), Error> {
        let mut mpv = self.mpv.lock().await;
        let _ = mpv.seek_relative(offset);
        Ok(())
    }

    /// Set mpv volume (0-100).
    pub async fn set_volume(self: &Arc<Self>, vol: i32) -> Result<(), Error> {
        let mut mpv = self.mpv.lock().await;
        let _ = mpv.set_volume(vol);
        Ok(())
    }

    /// Periodic poll: detect track advancement, update position and audio
    /// properties, drive PipeWire sample-rate switching, emit events. Called
    /// every 500ms by `App::event_loop` (phase 2.2c moves this to a
    /// `tokio::spawn`'d task on the core itself).
    pub async fn update_playback_info(self: &Arc<Self>) {
        use crate::app::state::PlaybackState;

        let (is_playing, is_active) = {
            let state = self.state.read().await;
            let pl = state.daemon.now_playing.state == PlaybackState::Playing;
            let active = pl || state.daemon.now_playing.state == PlaybackState::Paused;
            (pl, active)
        };

        if !is_active {
            return;
        }
        {
            let mut mpv = self.mpv.lock().await;
            if !mpv.is_running() {
                return;
            }
        }

        if is_playing {
            // Early advance: near end of track with no preloaded next.
            let (time_remaining, has_next) = {
                let state = self.state.read().await;
                let tr = state.daemon.now_playing.duration - state.daemon.now_playing.position;
                let hn = state
                    .daemon.queue_position
                    .map(|p| p + 1 < state.daemon.queue.len())
                    .unwrap_or(false);
                (tr, hn)
            };

            if has_next && time_remaining > 0.0 && time_remaining < 2.0 {
                let count_opt = {
                    let mut mpv = self.mpv.lock().await;
                    mpv.get_playlist_count().ok()
                };
                if let Some(count) = count_opt {
                    if count < 2 {
                        info!("Near end of track with no preloaded next — advancing early");
                        let _ = self.next_track().await;
                        return;
                    }
                }
            }

            // Re-preload if mpv lost the appended track.
            let count_opt = {
                let mut mpv = self.mpv.lock().await;
                mpv.get_playlist_count().ok()
            };
            if count_opt == Some(1) {
                let next_pos_opt = {
                    let state = self.state.read().await;
                    state.daemon.queue_position.and_then(|pos| {
                        if pos + 1 < state.daemon.queue.len() {
                            Some(pos)
                        } else {
                            None
                        }
                    })
                };
                if let Some(pos) = next_pos_opt {
                    debug!("Playlist count is 1, re-preloading next track");
                    self.preload_next_track(pos).await;
                }
            }

            // Detect mpv's gapless advance to next track.
            let mpv_pos_opt = {
                let mut mpv = self.mpv.lock().await;
                mpv.get_playlist_pos().ok().flatten()
            };
            if mpv_pos_opt == Some(1) {
                let advance_info = {
                    let state = self.state.read().await;
                    state.daemon.queue_position.and_then(|cur| {
                        let next = cur + 1;
                        if next < state.daemon.queue.len() {
                            state.daemon.queue.get(next).map(|s| (next, s.clone()))
                        } else {
                            None
                        }
                    })
                };
                if let Some((next_pos, song)) = advance_info {
                    info!("Gapless advancement to track {}", next_pos);
                    {
                        let mut state = self.state.write().await;
                        state.daemon.queue_position = Some(next_pos);
                        state.daemon.now_playing.song = Some(song.clone());
                        state.daemon.now_playing.position = 0.0;
                        state.daemon.now_playing.duration = song.duration.unwrap_or(0) as f64;
                    }
                    {
                        let mut mpv = self.mpv.lock().await;
                        let _ = mpv.playlist_remove(0);
                    }
                    self.preload_next_track(next_pos).await;
                    self.emit_now_playing().await;
                    return;
                }
            }

            // Track ended with no preload.
            let idle_opt = {
                let mut mpv = self.mpv.lock().await;
                mpv.is_idle().ok()
            };
            if idle_opt == Some(true) {
                info!("Track ended, advancing to next");
                let _ = self.next_track().await;
                return;
            }
        }

        // Update position from mpv.
        let pos_opt = {
            let mut mpv = self.mpv.lock().await;
            mpv.get_time_pos().ok()
        };
        if let Some(position) = pos_opt {
            let mut state = self.state.write().await;
            state.daemon.now_playing.position = position;
            // Position-only update broadcasts cheaply via PositionTick;
            // skip full NowPlayingChanged here to avoid re-render storm.
            drop(state);
            self.emit(DaemonEvent::PositionTick(position));
        }

        // Pull duration if not set yet.
        let need_duration = {
            let state = self.state.read().await;
            state.daemon.now_playing.duration <= 0.0
        };
        if need_duration {
            let dur_opt = {
                let mut mpv = self.mpv.lock().await;
                mpv.get_duration().ok()
            };
            if let Some(duration) = dur_opt {
                if duration > 0.0 {
                    let mut state = self.state.write().await;
                    state.daemon.now_playing.duration = duration;
                }
            }
        }

        // Pull audio properties — keep polling until valid.
        let need_sr = {
            let state = self.state.read().await;
            state.daemon.now_playing.sample_rate.is_none()
        };
        if need_sr {
            let (sr, bd, fmt, ch) = {
                let mut mpv = self.mpv.lock().await;
                (
                    mpv.get_sample_rate().ok().flatten(),
                    mpv.get_bit_depth().ok().flatten(),
                    mpv.get_audio_format().ok().flatten(),
                    mpv.get_channels().ok().flatten(),
                )
            };
            if let Some(rate) = sr {
                let need_switch = {
                    let pw = self.pipewire.lock().await;
                    pw.get_current_rate() != Some(rate)
                };
                if need_switch {
                    let mut pw = self.pipewire.lock().await;
                    info!("Sample rate change to {} Hz", rate);
                    if let Err(e) = pw.set_rate(rate).await {
                        warn!("Failed to set PipeWire sample rate: {}", e);
                    }
                } else {
                    debug!("Sample rate unchanged at {} Hz", rate);
                }
                let mut state = self.state.write().await;
                state.daemon.now_playing.sample_rate = Some(rate);
                state.daemon.now_playing.bit_depth = bd;
                state.daemon.now_playing.format = fmt;
                state.daemon.now_playing.channels = ch;
                drop(state);
                self.emit_now_playing().await;
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Queue ops (for IPC EnqueueSongs / RemoveFromQueue / ClearQueue / ShuffleQueue).
// The phase 2.4 input handlers route here through `DaemonClient::request`.
// ─────────────────────────────────────────────────────────────────────────

impl DaemonCore {
    /// Public emit hook for external mutators (e.g., `InProcessClient`
    /// after a queue rewrite that touches `state.daemon.queue` directly).
    pub async fn broadcast_queue_changed(self: &Arc<Self>) {
        self.emit_queue().await;
    }

    /// Move a queue item from `from` to `to`. Adjusts `queue_position`
    /// so the currently-playing track continues to refer to the same
    /// song after the reorder. No-op if either index is out of range.
    pub async fn move_queue_item(self: &Arc<Self>, from: usize, to: usize) {
        let mut state = self.state.write().await;
        let len = state.daemon.queue.len();
        if from >= len || to >= len || from == to {
            return;
        }
        let song = state.daemon.queue.remove(from);
        state.daemon.queue.insert(to, song);
        if let Some(cur) = state.daemon.queue_position {
            let new_cur = if cur == from {
                to
            } else if from < cur && to >= cur {
                cur - 1
            } else if from > cur && to <= cur {
                cur + 1
            } else {
                cur
            };
            state.daemon.queue_position = Some(new_cur);
        }
        drop(state);
        self.emit_queue().await;
    }

    /// Drain queue entries [0..queue_position]. Used by the "clear
    /// history" key in the queue page. After this call, `queue_position`
    /// becomes 0 (the currently-playing song is at the front).
    pub async fn clear_queue_history(self: &Arc<Self>) -> usize {
        let mut state = self.state.write().await;
        let Some(pos) = state.daemon.queue_position else {
            return 0;
        };
        if pos == 0 {
            return 0;
        }
        let removed = pos;
        state.daemon.queue.drain(0..pos);
        state.daemon.queue_position = Some(0);
        drop(state);
        self.emit_queue().await;
        removed
    }

    /// Shuffle the queue, preserving the currently-playing track at its
    /// position. No-op on an empty queue.
    pub async fn shuffle_queue(self: &Arc<Self>) {
        use rand::seq::SliceRandom;
        // Scope the !Send `thread_rng` so it's dropped before the
        // `emit_queue().await` below; otherwise the resulting future
        // is !Send and the broadcast server can't spawn it.
        {
            let mut state = self.state.write().await;
            if state.daemon.queue.is_empty() {
                return;
            }
            let mut rng = rand::thread_rng();
            match state.daemon.queue_position {
                Some(cur) if cur < state.daemon.queue.len() => {
                    let current = state.daemon.queue.remove(cur);
                    state.daemon.queue.shuffle(&mut rng);
                    state.daemon.queue.insert(cur, current);
                }
                _ => state.daemon.queue.shuffle(&mut rng),
            }
        }
        self.emit_queue().await;
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Library lazy-load methods that return their data inline (for IPC
// LoadAlbum / LoadPlaylist), as opposed to caching it like load_artist.
// ─────────────────────────────────────────────────────────────────────────

impl DaemonCore {
    /// Fetch the songs for an album. Empty `Vec` if not configured or
    /// fetch fails (the error is logged and pushed as a notification).
    pub async fn load_album_songs(self: &Arc<Self>, album_id: &str) -> Vec<crate::subsonic::models::Child> {
        let Some(client) = self.subsonic.read().await.clone() else { return Vec::new(); };
        match client.get_album(album_id).await {
            Ok((_album, songs)) => {
                {
                    let mut state = self.state.write().await;
                    crate::daemon::library::cache_insert(
                        &mut state.daemon.library.album_songs_cache,
                        album_id.to_string(),
                        songs.clone(),
                        crate::daemon::library::ALBUM_SONGS_CACHE_CAP,
                    );
                }
                self.emit(DaemonEvent::AlbumSongsChanged {
                    album_id: album_id.to_string(),
                    songs: songs.clone(),
                });
                songs
            }
            Err(e) => {
                error!("Failed to load album songs: {}", e);
                self.emit(DaemonEvent::Notification {
                    message: format!("Failed to load album: {}", e),
                    is_error: true,
                });
                Vec::new()
            }
        }
    }

    /// Fetch the songs for a playlist. Empty `Vec` if not configured or
    /// fetch fails (the error is logged and pushed as a notification).
    pub async fn load_playlist_songs(self: &Arc<Self>, playlist_id: &str) -> Vec<crate::subsonic::models::Child> {
        let Some(client) = self.subsonic.read().await.clone() else { return Vec::new(); };
        match client.get_playlist(playlist_id).await {
            Ok((_pl, songs)) => {
                {
                    let mut state = self.state.write().await;
                    crate::daemon::library::cache_insert(
                        &mut state.daemon.library.playlist_songs_cache,
                        playlist_id.to_string(),
                        songs.clone(),
                        crate::daemon::library::PLAYLIST_SONGS_CACHE_CAP,
                    );
                }
                self.emit(DaemonEvent::PlaylistSongsChanged {
                    playlist_id: playlist_id.to_string(),
                    songs: songs.clone(),
                });
                songs
            }
            Err(e) => {
                error!("Failed to load playlist songs: {}", e);
                self.emit(DaemonEvent::Notification {
                    message: format!("Failed to load playlist: {}", e),
                    is_error: true,
                });
                Vec::new()
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Config / settings: server, theme, cava. Each persists the config file
// and emits a `ConfigChanged` event so subscribers refresh their views.
// ─────────────────────────────────────────────────────────────────────────

impl DaemonCore {
    /// Replace the Subsonic server config and persist. Reinitialises the
    /// `SubsonicClient` and triggers the standard initial-data refresh.
    pub async fn update_server_config(
        self: &Arc<Self>,
        base_url: &str,
        username: &str,
        password: &str,
    ) -> Result<(), Error> {
        {
            let mut state = self.state.write().await;
            state.daemon.config.base_url = base_url.to_string();
            state.daemon.config.username = username.to_string();
            state.daemon.config.password = password.to_string();
            state.daemon.config.save_default().map_err(Error::Config)?;
        }

        // Build a fresh client and stash it.
        let new_client = SubsonicClient::new(base_url, username, password)
            .map_err(Error::Subsonic)?;
        *self.subsonic.write().await = Some(new_client);

        // Refetch initial data on the new server.
        self.refresh_starred().await;
        self.refresh_artists().await;
        self.refresh_playlists().await;

        self.emit_config_changed().await;
        Ok(())
    }

    /// Test a candidate Subsonic server config without persisting.
    /// Returns `(ok, message)` for display in the Server page status.
    pub async fn test_server_connection(
        self: &Arc<Self>,
        base_url: &str,
        username: &str,
        password: &str,
    ) -> (bool, String) {
        match SubsonicClient::new(base_url, username, password) {
            Ok(client) => match client.ping().await {
                Ok(()) => (true, "Connection OK".to_string()),
                Err(e) => (false, format!("Connection failed: {}", e)),
            },
            Err(e) => (false, format!("Invalid URL: {}", e)),
        }
    }

    /// Set the active theme by name and persist.
    pub async fn set_theme(self: &Arc<Self>, name: &str) -> Result<(), Error> {
        {
            let mut state = self.state.write().await;
            state.daemon.config.theme = name.to_string();
            state
                .daemon
                .config
                .save_default()
                .map_err(Error::Config)?;
        }
        self.emit_config_changed().await;
        Ok(())
    }

    /// Enable/disable cava and persist.
    pub async fn set_cava_enabled(self: &Arc<Self>, on: bool) -> Result<(), Error> {
        {
            let mut state = self.state.write().await;
            state.daemon.config.cava = on;
            state
                .daemon
                .config
                .save_default()
                .map_err(Error::Config)?;
        }
        self.emit_config_changed().await;
        Ok(())
    }

    /// Persist the daemon-mode preference. The setting controls whether
    /// the *next* TUI launch will attempt to spawn/connect a daemon;
    /// it does not affect the currently-running daemon.
    pub async fn set_daemon_enabled(self: &Arc<Self>, on: bool) -> Result<(), Error> {
        {
            let mut state = self.state.write().await;
            state.daemon.config.daemon = on;
            state
                .daemon
                .config
                .save_default()
                .map_err(Error::Config)?;
        }
        self.emit_config_changed().await;
        Ok(())
    }

    /// Set cava size (10..=80) and persist.
    pub async fn set_cava_size(self: &Arc<Self>, size: u8) -> Result<(), Error> {
        let clamped = size.clamp(10, 80);
        {
            let mut state = self.state.write().await;
            state.daemon.config.cava_size = clamped;
            state
                .daemon
                .config
                .save_default()
                .map_err(Error::Config)?;
        }
        self.emit_config_changed().await;
        Ok(())
    }
}

fn song_is_starred(daemon: &DaemonState, song_id: &str) -> bool {
    if let Some(s) = daemon.library.starred_songs.iter().find(|s| s.id == song_id) {
        return s.starred.is_some();
    }
    let all_cached = daemon
        .queue
        .iter()
        .chain(daemon.library.random_songs.iter())
        .chain(daemon.library.album_songs_cache.values().flatten())
        .chain(daemon.library.playlist_songs_cache.values().flatten());
    for s in all_cached {
        if s.id == song_id {
            return s.starred.is_some();
        }
    }
    false
}

fn apply_star_to_cached(daemon: &mut DaemonState, song_id: &str, starred: bool) {
    let marker = if starred { Some("1".to_string()) } else { None };
    let lists: [&mut Vec<crate::subsonic::models::Child>; 2] = [
        &mut daemon.queue,
        &mut daemon.library.random_songs,
    ];
    for list in lists {
        for song in list.iter_mut() {
            if song.id == song_id {
                song.starred = marker.clone();
            }
        }
    }
    for list in daemon.library.album_songs_cache.values_mut() {
        for song in list.iter_mut() {
            if song.id == song_id {
                song.starred = marker.clone();
            }
        }
    }
    for list in daemon.library.playlist_songs_cache.values_mut() {
        for song in list.iter_mut() {
            if song.id == song_id {
                song.starred = marker.clone();
            }
        }
    }
    if let Some(np) = daemon.now_playing.song.as_mut() {
        if np.id == song_id {
            np.starred = marker;
        }
    }
}
