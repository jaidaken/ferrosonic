//! Daemon core: owns mpv, the queue, the library cache, the event
//! broadcast, and config persistence.

use std::sync::Arc;

use tokio::sync::{broadcast, Mutex, RwLock};
use tracing::{debug, error, info, warn};

use crate::app::state::SharedDaemonState;
use crate::audio::mpv::MpvController;
use crate::audio::pipewire::PipeWireController;
use crate::config::Config;
use crate::daemon::persistence::QueueSnapshot;
use crate::daemon::state::DaemonState;
use crate::error::Error;
use crate::ipc::protocol::DaemonEvent;
use crate::subsonic::SubsonicClient;

/// Audio-handoff strategy for `play_queue_position`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayMode {
    /// `loadfile URL replace` — short mpv-internal gap; use for
    /// manual Next/Prev and explicit jumps within the current queue.
    Direct,
    /// Stop mpv immediately (audio device stays open, emits silence),
    /// pre-buffer the new file to disk, then `loadfile` the local
    /// copy. Use for queue replacement (album switch, shuffle library)
    /// so the new track starts cleanly with no mid-track choppiness.
    Buffered,
}

const EVENT_CHANNEL_CAPACITY: usize = 256;

pub struct DaemonCore {
    pub state: SharedDaemonState,
    pub mpv: Mutex<MpvController>,
    pub pipewire: Mutex<PipeWireController>,
    pub subsonic: RwLock<Option<SubsonicClient>>,
    pub event_tx: broadcast::Sender<DaemonEvent>,
    /// Trailing-edge debounce: `try_send(())` on every queue change;
    /// the persistence task drains, sleeps briefly, writes once.
    queue_save_tx: tokio::sync::mpsc::Sender<()>,
    /// Bounded at 64 entries, keyed `"<coverArt-id>@<size>"`.
    cover_art_cache: RwLock<std::collections::HashMap<String, Vec<u8>>>,
    /// Cancellation flag for the in-flight pre-buffer task. Replaced
    /// (and the old one flipped) on each new request so rapid track
    /// switches don't stack downloads.
    prebuffer_cancel: Mutex<Option<std::sync::Arc<std::sync::atomic::AtomicBool>>>,
    /// Holds recent `NamedTempFile` handles so the underlying inode
    /// stays alive while mpv still has it open. Bounded so old files
    /// eventually get unlinked.
    prebuffer_files: Mutex<Vec<std::sync::Arc<tempfile::NamedTempFile>>>,
}

impl DaemonCore {
    pub fn new(state: SharedDaemonState, config: &Config) -> Arc<Self> {
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
            cover_art_cache: RwLock::new(std::collections::HashMap::new()),
            prebuffer_cancel: Mutex::new(None),
            prebuffer_files: Mutex::new(Vec::new()),
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
                s.queue = snap.queue;
                s.queue_position = snap.position;
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
                        queue: s.queue.clone(),
                        position: s.queue_position,
                    }
                };
                if let Err(e) = snap.save() {
                    warn!("Queue persistence write failed: {}", e);
                }
            }
        })
    }

    /// Idempotent — no-ops if mpv is already running.
    pub async fn start_mpv(&self) -> Result<(), Error> {
        let mut mpv = self.mpv.lock().await;
        mpv.start().await.map_err(Into::into)
    }

    pub async fn quit_mpv(&self) {
        let mut mpv = self.mpv.lock().await;
        let _ = mpv.quit().await;
    }

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

    #[allow(dead_code)]
    pub fn subscribe(&self) -> broadcast::Receiver<DaemonEvent> {
        self.event_tx.subscribe()
    }

    fn emit(&self, event: DaemonEvent) {
        let _ = self.event_tx.send(event);
    }

    async fn emit_now_playing(&self) {
        let np = {
            let state = self.state.read().await;
            state.now_playing.clone()
        };
        self.emit(DaemonEvent::NowPlayingChanged(np));
    }

    async fn emit_queue(&self) {
        let (queue, position) = {
            let state = self.state.read().await;
            (state.queue.clone(), state.queue_position)
        };
        let _ = self.queue_save_tx.try_send(());
        self.emit(DaemonEvent::QueueChanged { queue, position });
    }

    /// Snapshot for a connecting client. Password is scrubbed — the
    /// TUI never talks to the Subsonic server directly.
    pub async fn snapshot(&self) -> DaemonState {
        let mut snap = {
            let state = self.state.read().await;
            state.clone()
        };
        snap.config.password.clear();
        snap.config.password_file = None;
        snap
    }

    async fn emit_config_changed(&self) {
        let mut cfg = {
            let state = self.state.read().await;
            state.config.clone()
        };
        cfg.password.clear();
        cfg.password_file = None;
        self.emit(DaemonEvent::ConfigChanged(cfg));
    }
}

impl DaemonCore {
    pub async fn refresh_starred(self: &Arc<Self>) {
        let Some(client) = self.subsonic.read().await.clone() else { return; };
        match client.get_starred_songs().await {
            Ok(songs) => {
                let mut state = self.state.write().await;
                state.library.starred_songs = songs.clone();
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
                state.library.random_songs = songs.clone();
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
                state.library.artists = artists.clone();
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
                state.library.playlists = playlists.clone();
                drop(state);
                info!("Loaded {} playlists", count);
                self.emit(DaemonEvent::PlaylistsChanged(playlists));
            }
            Err(e) => {
                error!("Failed to load playlists: {}", e);
            }
        }
    }

    pub async fn toggle_star_song(self: &Arc<Self>, song_id: &str) -> Result<bool, Error> {
        let currently_starred = {
            let state = self.state.read().await;
            song_is_starred(&*state, song_id)
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
            apply_star_to_cached(&mut *state, song_id, new_starred);
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
                    &mut state.library.albums_cache,
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

impl DaemonCore {
    pub async fn toggle_pause(self: &Arc<Self>) -> Result<(), Error> {
        use crate::app::state::PlaybackState;
        let (playback_state, queue_pos) = {
            let state = self.state.read().await;
            (state.now_playing.state, state.queue_position)
        };
        if playback_state == PlaybackState::Stopped {
            if let Some(pos) = queue_pos {
                return self.play_queue_position(pos, PlayMode::Direct).await;
            }
            return Ok(());
        }
        if playback_state != PlaybackState::Playing && playback_state != PlaybackState::Paused {
            return Ok(());
        }

        let mut mpv = self.mpv.lock().await;
        match mpv.toggle_pause().await {
            Ok(now_paused) => {
                drop(mpv);
                let mut state = self.state.write().await;
                state.now_playing.state = if now_paused {
                    PlaybackState::Paused
                } else {
                    PlaybackState::Playing
                };
                debug!("toggle_pause: now {:?}", state.now_playing.state);
                drop(state);
                self.emit_now_playing().await;
            }
            Err(e) => {
                error!("Failed to toggle pause: {}", e);
            }
        }
        Ok(())
    }

    pub async fn pause_playback(self: &Arc<Self>) -> Result<(), Error> {
        use crate::app::state::PlaybackState;
        {
            let state = self.state.read().await;
            if state.now_playing.state != PlaybackState::Playing {
                return Ok(());
            }
        }
        let mut mpv = self.mpv.lock().await;
        match mpv.pause().await {
            Ok(()) => {
                drop(mpv);
                let mut state = self.state.write().await;
                state.now_playing.state = PlaybackState::Paused;
                drop(state);
                self.emit_now_playing().await;
            }
            Err(e) => error!("Failed to pause: {}", e),
        }
        Ok(())
    }

    pub async fn resume_playback(self: &Arc<Self>) -> Result<(), Error> {
        use crate::app::state::PlaybackState;
        let (playback_state, queue_pos) = {
            let state = self.state.read().await;
            (state.now_playing.state, state.queue_position)
        };
        if playback_state == PlaybackState::Stopped {
            if let Some(pos) = queue_pos {
                return self.play_queue_position(pos, PlayMode::Direct).await;
            }
            return Ok(());
        }
        if playback_state != PlaybackState::Paused {
            return Ok(());
        }
        let mut mpv = self.mpv.lock().await;
        match mpv.resume().await {
            Ok(()) => {
                drop(mpv);
                let mut state = self.state.write().await;
                state.now_playing.state = PlaybackState::Playing;
                drop(state);
                self.emit_now_playing().await;
            }
            Err(e) => error!("Failed to resume: {}", e),
        }
        Ok(())
    }

    /// Manual skip. Ignores `repeat=One` (user wants to move).
    pub async fn next_track(self: &Arc<Self>) -> Result<(), Error> {
        use crate::app::state::PlaybackState;
        let (queue_len, current_pos, auto_continue, repeat) = {
            let state = self.state.read().await;
            (
                state.queue.len(),
                state.queue_position,
                state.config.auto_continue,
                state.config.repeat_mode,
            )
        };
        if queue_len == 0 {
            return Ok(());
        }
        let next_pos: Option<usize> = match current_pos {
            Some(p) => repeat.next_manual(p, queue_len),
            None => Some(0),
        };
        if let Some(p) = next_pos {
            return self.play_queue_position(p, PlayMode::Direct).await;
        }
        if auto_continue {
            info!("Queue ended, auto-continuing with random songs");
            if let Some(client) = self.subsonic.read().await.clone() {
                match client.get_random_songs().await {
                    Ok(songs) if !songs.is_empty() => {
                        let start_pos;
                        {
                            let mut state = self.state.write().await;
                            start_pos = state.queue.len();
                            state.queue.extend(songs);
                        }
                        self.emit_queue().await;
                        return self.play_queue_position(start_pos, PlayMode::Buffered).await;
                    }
                    Ok(_) => {
                        self.emit(DaemonEvent::Notification {
                            message: "Auto-continue: server returned no songs".to_string(),
                            is_error: true,
                        });
                    }
                    Err(e) => {
                        error!("Auto-continue fetch failed: {}", e);
                        self.emit(DaemonEvent::Notification {
                            message: format!("Auto-continue failed: {}", e),
                            is_error: true,
                        });
                    }
                }
            }
        }
        info!("Reached end of queue");
        let mut mpv = self.mpv.lock().await;
        let _ = mpv.stop().await;
        drop(mpv);
        let mut state = self.state.write().await;
        state.now_playing.state = PlaybackState::Stopped;
        state.now_playing.position = 0.0;
        drop(state);
        self.emit_now_playing().await;
        Ok(())
    }

    /// Auto-end advance. Honours `repeat=One` and `repeat=All`.
    pub async fn advance_auto(self: &Arc<Self>) -> Result<(), Error> {
        use crate::app::state::PlaybackState;
        let (queue_len, current_pos, auto_continue, repeat) = {
            let state = self.state.read().await;
            (
                state.queue.len(),
                state.queue_position,
                state.config.auto_continue,
                state.config.repeat_mode,
            )
        };
        if queue_len == 0 {
            return Ok(());
        }
        let next_pos: Option<usize> = match current_pos {
            Some(p) => repeat.next_auto(p, queue_len),
            None => Some(0),
        };
        if let Some(p) = next_pos {
            return self.play_queue_position(p, PlayMode::Direct).await;
        }
        if auto_continue {
            info!("Queue ended, auto-continuing with random songs");
            if let Some(client) = self.subsonic.read().await.clone() {
                match client.get_random_songs().await {
                    Ok(songs) if !songs.is_empty() => {
                        let start_pos;
                        {
                            let mut state = self.state.write().await;
                            start_pos = state.queue.len();
                            state.queue.extend(songs);
                        }
                        self.emit_queue().await;
                        return self.play_queue_position(start_pos, PlayMode::Buffered).await;
                    }
                    Ok(_) => {
                        self.emit(DaemonEvent::Notification {
                            message: "Auto-continue: server returned no songs".to_string(),
                            is_error: true,
                        });
                    }
                    Err(e) => {
                        error!("Auto-continue fetch failed: {}", e);
                        self.emit(DaemonEvent::Notification {
                            message: format!("Auto-continue failed: {}", e),
                            is_error: true,
                        });
                    }
                }
            }
        }
        info!("Reached end of queue");
        let mut mpv = self.mpv.lock().await;
        let _ = mpv.stop().await;
        drop(mpv);
        let mut state = self.state.write().await;
        state.now_playing.state = PlaybackState::Stopped;
        state.now_playing.position = 0.0;
        drop(state);
        self.emit_now_playing().await;
        Ok(())
    }

    /// Restarts current track if more than 3s in, else goes back one.
    pub async fn prev_track(self: &Arc<Self>) -> Result<(), Error> {
        let (queue_len, current_pos, position, repeat) = {
            let state = self.state.read().await;
            (
                state.queue.len(),
                state.queue_position,
                state.now_playing.position,
                state.config.repeat_mode,
            )
        };
        if queue_len == 0 {
            return Ok(());
        }
        if position < 3.0 {
            if let Some(pos) = current_pos {
                if pos > 0 {
                    return self.play_queue_position(pos - 1, PlayMode::Direct).await;
                }
                if let Some(wrap_to) = repeat.prev_wrap(queue_len) {
                    return self.play_queue_position(wrap_to, PlayMode::Direct).await;
                }
            }
            let mut mpv = self.mpv.lock().await;
            if let Err(e) = mpv.seek(0.0).await {
                error!("Failed to restart track: {}", e);
            } else {
                drop(mpv);
                let mut state = self.state.write().await;
                state.now_playing.position = 0.0;
            }
            return Ok(());
        }
        let mut mpv = self.mpv.lock().await;
        if let Err(e) = mpv.seek(0.0).await {
            error!("Failed to restart track: {}", e);
        } else {
            drop(mpv);
            let mut state = self.state.write().await;
            state.now_playing.position = 0.0;
        }
        Ok(())
    }

    pub async fn play_queue_position(self: &Arc<Self>, pos: usize, mode: PlayMode) -> Result<(), Error> {
        use crate::app::state::PlaybackState;
        let song = {
            let state = self.state.read().await;
            match state.queue.get(pos) {
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
            state.queue_position = Some(pos);
            state.now_playing.song = Some(song.clone());
            state.now_playing.state = PlaybackState::Playing;
            state.now_playing.position = 0.0;
            state.now_playing.duration = song.duration.unwrap_or(0) as f64;
            state.now_playing.sample_rate = None;
            state.now_playing.bit_depth = None;
            state.now_playing.format = None;
            state.now_playing.channels = None;
        }

        info!("Playing: {} (queue pos {}) mode={:?}", song.title, pos, mode);

        match mode {
            PlayMode::Direct => {
                let mut mpv = self.mpv.lock().await;
                if mpv.is_paused().await.unwrap_or(false) {
                    let _ = mpv.resume().await;
                }
                if let Err(e) = mpv.loadfile(&stream_url).await {
                    error!("Failed to play: {}", e);
                    drop(mpv);
                    self.emit(DaemonEvent::Notification {
                        message: format!("MPV error: {}", e),
                        is_error: true,
                    });
                    return Ok(());
                }
                drop(mpv);
                // Preload next now; mpv is loading new file.
                self.preload_next_track(pos).await;
            }
            PlayMode::Buffered => {
                // Cancel any in-flight prebuffer FIRST. If a previous
                // task is about to call `mpv.loadfile`, this races with
                // our `mpv.stop` below — by setting the cancel flag
                // first the task has a chance to bail before reaching
                // the mpv lock.
                use std::sync::atomic::Ordering;
                if let Some(prev) = self.prebuffer_cancel.lock().await.take() {
                    prev.store(true, Ordering::Relaxed);
                }
                {
                    let mut mpv = self.mpv.lock().await;
                    if mpv.is_paused().await.unwrap_or(false) {
                        let _ = mpv.resume().await;
                    }
                    // Stop current audio immediately. Audio device
                    // stays open (audio-stream-silence=yes) so the user
                    // hears actual silence, not a hardware re-init.
                    if mpv.is_running() && !mpv.is_idle().await.unwrap_or(true) {
                        let _ = mpv.stop().await;
                    }
                }
                // Pre-buffer in background; preload-next deferred to
                // the same task so we don't append to a soon-to-be-
                // replaced playlist.
                self.prebuffer_and_load(stream_url, pos).await;
            }
        }

        self.emit_now_playing().await;
        self.emit_queue().await;

        // Fast probe loop: poll mpv every 50ms for up to ~4s for audio
        // properties. mpv usually has them within 200-500ms after
        // loadfile; without this the 500ms backstop tick is the only
        // path and the quality row visibly lags.
        let core = self.clone();
        tokio::spawn(async move {
            for _ in 0..80 {
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                let still_missing = {
                    let state = core.state.read().await;
                    state.now_playing.sample_rate.is_none()
                };
                if !still_missing {
                    return;
                }
                if core.fetch_audio_properties().await {
                    return;
                }
            }
        });

        Ok(())
    }

    /// Smooth track swap: stream the new URL to a local temp file
    /// while the currently-playing mpv source keeps going. When the
    /// pre-buffer threshold is met (or the download finishes for a
    /// small file), point mpv at the local file via `loadfile`. Old
    /// audio stays continuous up to the loadfile moment; mpv reads
    /// from disk and starts decoding immediately.
    async fn prebuffer_and_load(self: &Arc<Self>, url: String, preload_pos: usize) {
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::Arc as StdArc;

        // Cancel any prior pre-buffer so we don't leave parallel
        // downloads writing temp files.
        if let Some(old) = self.prebuffer_cancel.lock().await.take() {
            old.store(true, Ordering::Relaxed);
        }

        let cancel = StdArc::new(AtomicBool::new(false));
        *self.prebuffer_cancel.lock().await = Some(cancel.clone());

        let temp = match tempfile::Builder::new()
            .prefix("ferrosonic-prebuf-")
            .suffix(".dat")
            .tempfile()
        {
            Ok(t) => StdArc::new(t),
            Err(e) => {
                error!("Pre-buffer: temp file create failed ({}); falling back to direct loadfile", e);
                let mut mpv = self.mpv.lock().await;
                let _ = mpv.loadfile(&url).await;
                return;
            }
        };

        // Bound the keep-alive list so old prebuf files eventually
        // unlink. Two slots is plenty: the one mpv is currently
        // reading + the one being prepared.
        {
            let mut files = self.prebuffer_files.lock().await;
            files.push(temp.clone());
            while files.len() > 2 {
                files.remove(0);
            }
        }

        let core = self.clone();
        let cancel_task = cancel.clone();
        let temp_task = temp.clone();

        tokio::spawn(async move {
            use futures::StreamExt;
            use std::io::Write;

            const PREBUFFER_THRESHOLD: usize = 512 * 1024;

            let path = temp_task.path().to_path_buf();
            let path_str = path.to_string_lossy().to_string();
            let start = std::time::Instant::now();

            let resp = match reqwest::Client::new().get(&url).send().await {
                Ok(r) => r,
                Err(e) => {
                    error!("Pre-buffer fetch failed: {}", e);
                    let mut mpv = core.mpv.lock().await;
                    let _ = mpv.loadfile(&url).await;
                    return;
                }
            };

            let mut file = match std::fs::File::create(&path) {
                Ok(f) => f,
                Err(e) => {
                    error!("Pre-buffer file open failed: {}", e);
                    let mut mpv = core.mpv.lock().await;
                    let _ = mpv.loadfile(&url).await;
                    return;
                }
            };

            let mut bytes_written: usize = 0;
            let mut triggered = false;
            let mut stream = resp.bytes_stream();

            while let Some(chunk) = stream.next().await {
                if cancel_task.load(Ordering::Relaxed) {
                    debug!("Pre-buffer cancelled at {} KB", bytes_written / 1024);
                    return;
                }
                let chunk = match chunk {
                    Ok(c) => c,
                    Err(e) => {
                        error!("Pre-buffer stream error: {}", e);
                        if !triggered {
                            let mut mpv = core.mpv.lock().await;
                            let _ = mpv.loadfile(&url).await;
                        }
                        return;
                    }
                };
                if let Err(e) = file.write_all(&chunk) {
                    error!("Pre-buffer write error: {}", e);
                    return;
                }
                bytes_written += chunk.len();

                if !triggered && bytes_written >= PREBUFFER_THRESHOLD {
                    triggered = true;
                    let _ = file.flush();
                    info!(
                        "Pre-buffer threshold reached ({} KB in {:?}); loading {}",
                        bytes_written / 1024,
                        start.elapsed(),
                        path_str
                    );
                    {
                        let mut mpv = core.mpv.lock().await;
                        // Re-check cancel with the mpv lock held —
                        // a newer switch may have set it between the
                        // top-of-loop check and now.
                        if cancel_task.load(Ordering::Relaxed) {
                            debug!("Pre-buffer cancelled before loadfile");
                            return;
                        }
                        if let Err(e) = mpv.loadfile(&path_str).await {
                            error!("Pre-buffer loadfile failed: {}", e);
                            return;
                        }
                    }
                    if cancel_task.load(Ordering::Relaxed) {
                        return;
                    }
                    // Now that the new file is the current entry,
                    // queue up the gapless preload for the song after.
                    core.preload_next_track(preload_pos).await;
                }
            }

            let _ = file.flush();
            if !triggered {
                info!(
                    "Pre-buffer download complete without hitting threshold ({} KB); loading",
                    bytes_written / 1024
                );
                {
                    let mut mpv = core.mpv.lock().await;
                    if cancel_task.load(Ordering::Relaxed) {
                        debug!("Pre-buffer cancelled before final loadfile");
                        return;
                    }
                    if let Err(e) = mpv.loadfile(&path_str).await {
                        error!("Pre-buffer loadfile failed: {}", e);
                        return;
                    }
                }
                if cancel_task.load(Ordering::Relaxed) {
                    return;
                }
                core.preload_next_track(preload_pos).await;
            } else {
                info!(
                    "Pre-buffer download finished: {} KB total in {:?}",
                    bytes_written / 1024,
                    start.elapsed()
                );
            }
        });
    }

    /// Repeat-aware: loads current for One, wraps for All, no-ops
    /// at the end for Off.
    pub async fn preload_next_track(self: &Arc<Self>, current_pos: usize) {
        let next_song = {
            let state = self.state.read().await;
            let queue_len = state.queue.len();
            let target = state
                .config
                .repeat_mode
                .next_auto(current_pos, queue_len);
            match target.and_then(|p| state.queue.get(p)) {
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
        if let Err(e) = mpv.loadfile_append(&url).await {
            debug!("Failed to pre-load next track: {}", e);
        } else if let Ok(count) = mpv.get_playlist_count().await {
            if count < 2 {
                warn!("Preload may have failed: playlist count is {} (expected 2)", count);
            } else {
                debug!("Preload confirmed: playlist count is {}", count);
            }
        }
    }

    pub async fn stop_playback(self: &Arc<Self>) -> Result<(), Error> {
        use crate::app::state::PlaybackState;
        {
            let mut mpv = self.mpv.lock().await;
            if let Err(e) = mpv.stop().await {
                error!("Failed to stop: {}", e);
            }
        }
        let mut state = self.state.write().await;
        state.now_playing.state = PlaybackState::Stopped;
        state.now_playing.song = None;
        state.now_playing.position = 0.0;
        state.now_playing.duration = 0.0;
        state.now_playing.sample_rate = None;
        state.now_playing.bit_depth = None;
        state.now_playing.format = None;
        state.now_playing.channels = None;
        state.queue.clear();
        state.queue_position = None;
        drop(state);
        self.emit_now_playing().await;
        self.emit_queue().await;
        Ok(())
    }

    /// MPRIS / Stop-button semantics: halt playback but keep the queue
    /// and current selection intact so Play can resume the same track.
    pub async fn stop_keep_queue(self: &Arc<Self>) -> Result<(), Error> {
        use crate::app::state::PlaybackState;
        {
            let mut mpv = self.mpv.lock().await;
            if let Err(e) = mpv.stop().await {
                error!("Failed to stop: {}", e);
            }
        }
        {
            let mut state = self.state.write().await;
            state.now_playing.state = PlaybackState::Stopped;
            state.now_playing.position = 0.0;
        }
        self.emit_now_playing().await;
        Ok(())
    }

    /// Stop mpv without touching the queue.
    pub async fn halt_keep_queue(self: &Arc<Self>) {
        use crate::app::state::PlaybackState;
        {
            let mut mpv = self.mpv.lock().await;
            if let Err(e) = mpv.stop().await {
                error!("Failed to stop: {}", e);
            }
        }
        {
            let mut state = self.state.write().await;
            state.now_playing.state = PlaybackState::Stopped;
            state.now_playing.song = None;
            state.now_playing.position = 0.0;
            state.now_playing.duration = 0.0;
            state.now_playing.sample_rate = None;
            state.now_playing.bit_depth = None;
            state.now_playing.format = None;
            state.now_playing.channels = None;
        }
        self.emit_now_playing().await;
    }

    pub async fn seek(self: &Arc<Self>, pos: f64) -> Result<(), Error> {
        let mut mpv = self.mpv.lock().await;
        if let Err(e) = mpv.seek(pos).await {
            warn!("Seek failed: {}", e);
            return Ok(());
        }
        drop(mpv);
        let mut state = self.state.write().await;
        state.now_playing.position = pos;
        Ok(())
    }

    pub async fn seek_relative(self: &Arc<Self>, offset: f64) -> Result<(), Error> {
        let mut mpv = self.mpv.lock().await;
        let _ = mpv.seek_relative(offset).await;
        Ok(())
    }

    pub async fn set_volume(self: &Arc<Self>, vol: i32) -> Result<(), Error> {
        let mut mpv = self.mpv.lock().await;
        let _ = mpv.set_volume(vol).await;
        Ok(())
    }

    /// Query mpv for sample rate / bit depth / format / channels and,
    /// if available, write them into state, drive the PipeWire rate
    /// switch, and emit `NowPlayingChanged`. Returns `true` when audio
    /// properties were populated this call.
    async fn fetch_audio_properties(self: &Arc<Self>) -> bool {
        let (sr, bd, fmt, ch) = {
            let mut mpv = self.mpv.lock().await;
            (
                mpv.get_sample_rate().await.ok().flatten(),
                mpv.get_bit_depth().await.ok().flatten(),
                mpv.get_audio_format().await.ok().flatten(),
                mpv.get_channels().await.ok().flatten(),
            )
        };
        let Some(rate) = sr else {
            return false;
        };
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
        }
        {
            let mut state = self.state.write().await;
            state.now_playing.sample_rate = Some(rate);
            state.now_playing.bit_depth = bd;
            state.now_playing.format = fmt;
            state.now_playing.channels = ch;
        }
        self.emit_now_playing().await;
        true
    }

    /// Periodic poll (500ms): detect track advance, update position +
    /// audio properties, drive PipeWire rate switching, emit events.
    pub async fn update_playback_info(self: &Arc<Self>) {
        use crate::app::state::PlaybackState;

        let (is_playing, is_active) = {
            let state = self.state.read().await;
            let pl = state.now_playing.state == PlaybackState::Playing;
            let active = pl || state.now_playing.state == PlaybackState::Paused;
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
            let (time_remaining, has_next) = {
                let state = self.state.read().await;
                let tr = state.now_playing.duration - state.now_playing.position;
                let hn = state
                    .queue_position
                    .map(|p| p + 1 < state.queue.len())
                    .unwrap_or(false);
                (tr, hn)
            };

            if has_next && time_remaining > 0.0 && time_remaining < 2.0 {
                let count_opt = {
                    let mut mpv = self.mpv.lock().await;
                    mpv.get_playlist_count().await.ok()
                };
                if let Some(count) = count_opt {
                    if count < 2 {
                        info!("Near end of track with no preloaded next — advancing early");
                        let _ = self.next_track().await;
                        return;
                    }
                }
            }

            let count_opt = {
                let mut mpv = self.mpv.lock().await;
                mpv.get_playlist_count().await.ok()
            };
            if count_opt == Some(1) {
                let cur_pos_opt = {
                    let state = self.state.read().await;
                    state.queue_position
                };
                if let Some(pos) = cur_pos_opt {
                    debug!("Playlist count is 1, re-preloading next track");
                    self.preload_next_track(pos).await;
                }
            }

            let mpv_pos_opt = {
                let mut mpv = self.mpv.lock().await;
                mpv.get_playlist_pos().await.ok().flatten()
            };
            if mpv_pos_opt == Some(1) {
                let advance_info = {
                    let state = self.state.read().await;
                    let queue_len = state.queue.len();
                    let repeat = state.config.repeat_mode;
                    state.queue_position.and_then(|cur| {
                        repeat
                            .next_auto(cur, queue_len)
                            .and_then(|n| state.queue.get(n).map(|s| (n, s.clone())))
                    })
                };
                if let Some((next_pos, song)) = advance_info {
                    info!("Gapless advancement to track {}", next_pos);
                    {
                        let mut state = self.state.write().await;
                        state.queue_position = Some(next_pos);
                        state.now_playing.song = Some(song.clone());
                        state.now_playing.position = 0.0;
                        state.now_playing.duration = song.duration.unwrap_or(0) as f64;
                    }
                    {
                        let mut mpv = self.mpv.lock().await;
                        let _ = mpv.playlist_remove(0).await;
                    }
                    self.preload_next_track(next_pos).await;
                    self.emit_now_playing().await;
                    // queue_position changed; clients use it to derive
                    // current_song(), so without this the play-indicator
                    // sticks on the previous track until the next manual
                    // queue mutation.
                    self.emit_queue().await;
                    return;
                }
            }

            let idle_opt = {
                let mut mpv = self.mpv.lock().await;
                mpv.is_idle().await.ok()
            };
            if idle_opt == Some(true) {
                info!("Track ended, advancing to next");
                let _ = self.advance_auto().await;
                return;
            }
        }

        let pos_opt = {
            let mut mpv = self.mpv.lock().await;
            mpv.get_time_pos().await.ok()
        };
        if let Some(position) = pos_opt {
            let mut state = self.state.write().await;
            state.now_playing.position = position;
            drop(state);
            self.emit(DaemonEvent::PositionTick(position));
        }

        let need_duration = {
            let state = self.state.read().await;
            state.now_playing.duration <= 0.0
        };
        if need_duration {
            let dur_opt = {
                let mut mpv = self.mpv.lock().await;
                mpv.get_duration().await.ok()
            };
            if let Some(duration) = dur_opt {
                if duration > 0.0 {
                    let mut state = self.state.write().await;
                    state.now_playing.duration = duration;
                }
            }
        }

        // Audio properties — backstop poll. Fast probe (spawned from
        // play_queue_position) usually picks them up before this tick.
        let need_sr = {
            let state = self.state.read().await;
            state.now_playing.sample_rate.is_none()
        };
        if need_sr {
            let _ = self.fetch_audio_properties().await;
        }
    }
}

impl DaemonCore {
    pub async fn broadcast_queue_changed(self: &Arc<Self>) {
        self.emit_queue().await;
    }

    /// Reorder; `queue_position` is adjusted to keep pointing at the
    /// same song.
    pub async fn move_queue_item(self: &Arc<Self>, from: usize, to: usize) {
        let mut state = self.state.write().await;
        let len = state.queue.len();
        if from >= len || to >= len || from == to {
            return;
        }
        let song = state.queue.remove(from);
        state.queue.insert(to, song);
        if let Some(cur) = state.queue_position {
            let new_cur = if cur == from {
                to
            } else if from < cur && to >= cur {
                cur - 1
            } else if from > cur && to <= cur {
                cur + 1
            } else {
                cur
            };
            state.queue_position = Some(new_cur);
        }
        drop(state);
        self.emit_queue().await;
    }

    /// Drain entries before `queue_position`. Returns count removed.
    pub async fn clear_queue_history(self: &Arc<Self>) -> usize {
        let mut state = self.state.write().await;
        let Some(pos) = state.queue_position else {
            return 0;
        };
        if pos == 0 {
            return 0;
        }
        let removed = pos;
        state.queue.drain(0..pos);
        state.queue_position = Some(0);
        drop(state);
        self.emit_queue().await;
        removed
    }

    pub async fn shuffle_library(self: &Arc<Self>) -> Result<(), Error> {
        let Some(client) = self.subsonic.read().await.clone() else {
            return Ok(());
        };
        let songs = match client.get_random_songs().await {
            Ok(s) if !s.is_empty() => s,
            Ok(_) => return Ok(()),
            Err(e) => {
                error!("Failed to load random songs: {}", e);
                self.emit(DaemonEvent::Notification {
                    message: format!("Failed to shuffle library: {}", e),
                    is_error: true,
                });
                return Ok(());
            }
        };
        {
            let mut state = self.state.write().await;
            state.library.random_songs = songs.clone();
            state.queue = songs.clone();
            state.queue_position = None;
        }
        self.emit(DaemonEvent::RandomChanged(songs));
        self.emit_queue().await;
        self.play_queue_position(0, PlayMode::Buffered).await
    }

    /// Shuffle preserving the currently-playing track in place.
    pub async fn shuffle_queue(self: &Arc<Self>) {
        use rand::seq::SliceRandom;
        // Scope `thread_rng` (!Send) out of the await below.
        {
            let mut state = self.state.write().await;
            if state.queue.is_empty() {
                return;
            }
            let mut rng = rand::thread_rng();
            match state.queue_position {
                Some(cur) if cur < state.queue.len() => {
                    let current = state.queue.remove(cur);
                    state.queue.shuffle(&mut rng);
                    state.queue.insert(cur, current);
                }
                _ => state.queue.shuffle(&mut rng),
            }
        }
        self.emit_queue().await;
    }
}

impl DaemonCore {
    pub async fn load_album_songs(self: &Arc<Self>, album_id: &str) -> Vec<crate::subsonic::models::Child> {
        let Some(client) = self.subsonic.read().await.clone() else { return Vec::new(); };
        match client.get_album(album_id).await {
            Ok((_album, songs)) => {
                {
                    let mut state = self.state.write().await;
                    crate::daemon::library::cache_insert(
                        &mut state.library.album_songs_cache,
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

    pub async fn search(
        self: &Arc<Self>,
        query: &str,
        artist_count: u32,
        album_count: u32,
        song_count: u32,
    ) -> crate::subsonic::models::SearchResult3 {
        let Some(client) = self.subsonic.read().await.clone() else {
            return Default::default();
        };
        match client
            .search3(query, artist_count, album_count, song_count)
            .await
        {
            Ok(r) => r,
            Err(e) => {
                error!("search3 failed: {}", e);
                Default::default()
            }
        }
    }

    pub async fn load_playlist_songs(self: &Arc<Self>, playlist_id: &str) -> Vec<crate::subsonic::models::Child> {
        let Some(client) = self.subsonic.read().await.clone() else { return Vec::new(); };
        match client.get_playlist(playlist_id).await {
            Ok((_pl, songs)) => {
                {
                    let mut state = self.state.write().await;
                    crate::daemon::library::cache_insert(
                        &mut state.library.playlist_songs_cache,
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

impl DaemonCore {
    pub async fn update_server_config(
        self: &Arc<Self>,
        base_url: &str,
        username: &str,
        password: &str,
    ) -> Result<(), Error> {
        {
            let mut state = self.state.write().await;
            state.config.base_url = base_url.to_string();
            state.config.username = username.to_string();
            state.config.password = password.to_string();
            state.config.save_default().map_err(Error::Config)?;
        }

        let new_client = SubsonicClient::new(base_url, username, password)
            .map_err(Error::Subsonic)?;
        *self.subsonic.write().await = Some(new_client);

        self.refresh_starred().await;
        self.refresh_artists().await;
        self.refresh_playlists().await;

        self.emit_config_changed().await;
        Ok(())
    }

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

    pub async fn set_theme(self: &Arc<Self>, name: &str) -> Result<(), Error> {
        {
            let mut state = self.state.write().await;
            state.config.theme = name.to_string();
            state
                .config
                .save_default()
                .map_err(Error::Config)?;
        }
        self.emit_config_changed().await;
        Ok(())
    }

    pub async fn set_cava_enabled(self: &Arc<Self>, on: bool) -> Result<(), Error> {
        {
            let mut state = self.state.write().await;
            state.config.cava = on;
            state
                .config
                .save_default()
                .map_err(Error::Config)?;
        }
        self.emit_config_changed().await;
        Ok(())
    }

    /// Takes effect on the next TUI launch.
    pub async fn set_daemon_enabled(self: &Arc<Self>, on: bool) -> Result<(), Error> {
        {
            let mut state = self.state.write().await;
            state.config.daemon = on;
            state
                .config
                .save_default()
                .map_err(Error::Config)?;
        }
        self.emit_config_changed().await;
        Ok(())
    }

    pub async fn set_auto_continue(self: &Arc<Self>, on: bool) -> Result<(), Error> {
        {
            let mut state = self.state.write().await;
            state.config.auto_continue = on;
            state.config.save_default().map_err(Error::Config)?;
        }
        self.emit_config_changed().await;
        Ok(())
    }

    /// Re-preloads the new auto-advance target so gapless picks up
    /// the mode change at the next track boundary.
    pub async fn set_repeat_mode(self: &Arc<Self>, mode: crate::config::RepeatMode) -> Result<(), Error> {
        let cur_pos = {
            let mut state = self.state.write().await;
            state.config.repeat_mode = mode;
            state.config.save_default().map_err(Error::Config)?;
            state.queue_position
        };
        self.emit(DaemonEvent::RepeatModeChanged(mode));
        self.emit_config_changed().await;
        if let Some(pos) = cur_pos {
            let mut mpv = self.mpv.lock().await;
            if let Ok(count) = mpv.get_playlist_count().await {
                if count > 1 {
                    let _ = mpv.playlist_remove(1).await;
                }
            }
            drop(mpv);
            self.preload_next_track(pos).await;
        }
        Ok(())
    }

    pub async fn set_cover_art_enabled(self: &Arc<Self>, on: bool) -> Result<(), Error> {
        {
            let mut state = self.state.write().await;
            state.config.cover_art = on;
            state.config.save_default().map_err(Error::Config)?;
        }
        self.emit_config_changed().await;
        Ok(())
    }

    pub async fn set_cover_art_size(self: &Arc<Self>, size: u8) -> Result<(), Error> {
        let clamped = size.clamp(8, 24);
        {
            let mut state = self.state.write().await;
            state.config.cover_art_size = clamped;
            state.config.save_default().map_err(Error::Config)?;
        }
        self.emit_config_changed().await;
        Ok(())
    }

    /// Returns empty on error so the caller renders no art.
    pub async fn get_cover_art(self: &Arc<Self>, id: &str, size: u32) -> Vec<u8> {
        let key = format!("{}@{}", id, size);
        {
            let cache = self.cover_art_cache.read().await;
            if let Some(bytes) = cache.get(&key) {
                return bytes.clone();
            }
        }
        let Some(client) = self.subsonic.read().await.clone() else {
            return Vec::new();
        };
        match client.get_cover_art(id, size).await {
            Ok(bytes) => {
                let mut cache = self.cover_art_cache.write().await;
                if cache.len() >= 64 {
                    if let Some(k) = cache.keys().next().cloned() {
                        cache.remove(&k);
                    }
                }
                cache.insert(key, bytes.clone());
                bytes
            }
            Err(e) => {
                error!("get_cover_art failed for {}: {}", id, e);
                Vec::new()
            }
        }
    }

    pub async fn set_cava_size(self: &Arc<Self>, size: u8) -> Result<(), Error> {
        let clamped = size.clamp(10, 80);
        {
            let mut state = self.state.write().await;
            state.config.cava_size = clamped;
            state
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
