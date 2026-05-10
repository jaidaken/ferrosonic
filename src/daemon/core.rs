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
use crate::error::Error;
use crate::ipc::protocol::{DaemonEvent, LibrarySection};
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

        Arc::new(Self {
            state,
            mpv: Mutex::new(MpvController::new()),
            pipewire: Mutex::new(PipeWireController::new()),
            subsonic: RwLock::new(subsonic),
            event_tx,
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
}

// ─────────────────────────────────────────────────────────────────────────
// Library fetches (was App's repo.rs).
// All methods read self.subsonic, write to self.state.daemon.library, emit
// LibraryChanged events, push notifications on error.
// ─────────────────────────────────────────────────────────────────────────

impl DaemonCore {
    pub async fn refresh_starred(self: &Arc<Self>) {
        let client = self.subsonic.read().await;
        let Some(ref client) = *client else {
            return;
        };
        match client.get_starred_songs().await {
            Ok(songs) => {
                let mut state = self.state.write().await;
                state.daemon.library.starred_songs = songs;
                drop(state);
                self.emit(DaemonEvent::LibraryChanged(LibrarySection::Starred));
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
        let client = self.subsonic.read().await;
        let Some(ref client) = *client else {
            return;
        };
        match client.get_random_songs().await {
            Ok(songs) => {
                let mut state = self.state.write().await;
                state.daemon.library.random_songs = songs;
                drop(state);
                self.emit(DaemonEvent::LibraryChanged(LibrarySection::Random));
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
        let client = self.subsonic.read().await;
        let Some(ref client) = *client else {
            return;
        };
        match client.get_artists().await {
            Ok(artists) => {
                let mut state = self.state.write().await;
                let count = artists.len();
                state.daemon.library.artists = artists;
                drop(state);
                info!("Loaded {} artists", count);
                self.emit(DaemonEvent::LibraryChanged(LibrarySection::Artists));
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
        let client = self.subsonic.read().await;
        let Some(ref client) = *client else {
            return;
        };
        match client.get_playlists().await {
            Ok(playlists) => {
                let mut state = self.state.write().await;
                let count = playlists.len();
                state.daemon.library.playlists = playlists;
                drop(state);
                info!("Loaded {} playlists", count);
                self.emit(DaemonEvent::LibraryChanged(LibrarySection::Playlists));
            }
            Err(e) => {
                error!("Failed to load playlists: {}", e);
                // Don't show error for playlists if artists loaded
            }
        }
    }

    #[allow(dead_code)] // wired up in phase 2.4 via DaemonRequest::LoadArtist
    pub async fn load_artist(self: &Arc<Self>, artist_id: &str) {
        let client = self.subsonic.read().await;
        let Some(ref client) = *client else {
            return;
        };
        match client.get_artist(artist_id).await {
            Ok((_artist, albums)) => {
                let mut state = self.state.write().await;
                let count = albums.len();
                state
                    .daemon
                    .library
                    .albums_cache
                    .insert(artist_id.to_string(), albums);
                drop(state);
                info!("Loaded {} albums for {}", count, artist_id);
                self.emit(DaemonEvent::LibraryChanged(LibrarySection::Albums));
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
                self.emit(DaemonEvent::NowPlayingChanged);
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
                self.emit(DaemonEvent::NowPlayingChanged);
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
                self.emit(DaemonEvent::NowPlayingChanged);
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
                self.emit(DaemonEvent::NowPlayingChanged);
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
            let client_lock = self.subsonic.read().await;
            let Some(ref client) = *client_lock else {
                return Ok(());
            };
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
        self.emit(DaemonEvent::NowPlayingChanged);
        self.emit(DaemonEvent::QueueChanged);
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
            let client_lock = self.subsonic.read().await;
            let Some(ref client) = *client_lock else {
                return;
            };
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
        self.emit(DaemonEvent::NowPlayingChanged);
        self.emit(DaemonEvent::QueueChanged);
        Ok(())
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

    /// One-shot "play this stream URL now": resume if paused, then loadfile.
    /// Used by single-song click flows (artist-page album song click etc.)
    /// that want to play a song without rewriting the queue. Note: in
    /// phase 6 these flows should be rewritten as proper queue ops.
    pub async fn play_url_now(self: &Arc<Self>, url: &str) {
        let mut mpv = self.mpv.lock().await;
        if mpv.is_paused().unwrap_or(false) {
            let _ = mpv.resume();
        }
        if let Err(e) = mpv.loadfile(url) {
            error!("Failed to play: {}", e);
        }
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
            let mpv = self.mpv.lock().await;
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
                    self.emit(DaemonEvent::NowPlayingChanged);
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
                    if let Err(e) = pw.set_rate(rate) {
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
                self.emit(DaemonEvent::NowPlayingChanged);
            }
        }
    }
}
