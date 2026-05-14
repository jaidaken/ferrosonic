//! Daemon core: owns mpv, queue, library cache, event broadcast, config persistence. Lock order: state then subsonic then mpv then pipewire then prebuffer_cancel then prebuffer_loading then prebuffer_files then last_loadfile then last_preload_attempt then cover_art_cache. Authoritative table: docs/LOCK-ORDER.md.

use std::sync::atomic::{AtomicBool, Ordering};
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

const EVENT_CHANNEL_CAPACITY: usize = 32;

/// Drop clears prebuffer_loading if dispatch_play is cancelled before the spawn task takes over.
struct LoadingFlagOwner {
    flag: Option<Arc<AtomicBool>>,
}

impl LoadingFlagOwner {
    fn new(flag: Arc<AtomicBool>) -> Self {
        Self { flag: Some(flag) }
    }
    fn disarm(&mut self) {
        self.flag = None;
    }
}

impl Drop for LoadingFlagOwner {
    fn drop(&mut self) {
        if let Some(f) = self.flag.take() {
            f.store(false, Ordering::Release);
        }
    }
}

/// RAII clear for `prebuffer_loading`. Drop clears the flag unless
/// `disarm()` was called (cancel paths leave the gate to a newer task).
struct PrebufferGate {
    flag: Arc<AtomicBool>,
    armed: std::cell::Cell<bool>,
}

impl PrebufferGate {
    fn new(flag: Arc<AtomicBool>) -> Self {
        Self {
            flag,
            armed: std::cell::Cell::new(true),
        }
    }
    fn disarm(&self) {
        self.armed.set(false);
    }
}

impl Drop for PrebufferGate {
    fn drop(&mut self) {
        if self.armed.get() {
            self.flag.store(false, Ordering::Release);
        }
    }
}

/// Drop-time cleanup of this task's slot in prebuffer_cancel; spawns a tiny task to take the async mutex.
struct CancelSlotCleaner {
    core: Arc<DaemonCore>,
    own: Arc<AtomicBool>,
    armed: std::cell::Cell<bool>,
}

impl CancelSlotCleaner {
    fn new(core: Arc<DaemonCore>, own: Arc<AtomicBool>) -> Self {
        Self {
            core,
            own,
            armed: std::cell::Cell::new(true),
        }
    }
    fn disarm(&self) {
        self.armed.set(false);
    }
}

impl Drop for CancelSlotCleaner {
    fn drop(&mut self) {
        if !self.armed.get() {
            return;
        }
        let core = self.core.clone();
        let own = self.own.clone();
        tokio::spawn(async move {
            let mut slot = core.prebuffer_cancel.lock().await;
            if let Some(current) = slot.as_ref() {
                if Arc::ptr_eq(current, &own) {
                    *slot = None;
                }
            }
        });
    }
}

pub struct DaemonCore {
    pub state: SharedDaemonState,
    pub mpv: Mutex<MpvController>,
    pub pipewire: Mutex<PipeWireController>,
    pub subsonic: RwLock<Option<SubsonicClient>>,
    pub event_tx: broadcast::Sender<DaemonEvent>,
    /// Trailing-edge debounce: `try_send(())` on every queue change;
    /// the persistence task drains, sleeps briefly, writes once.
    queue_save_tx: tokio::sync::mpsc::Sender<()>,
    /// Bounded at `COVER_ART_CACHE_CAP`, keyed `"<coverArt-id>@<size>"`.
    cover_art_cache: RwLock<crate::daemon::library::LruCache<Vec<u8>>>,
    /// Cancellation flag for the in-flight pre-buffer task. Replaced
    /// (and the old one flipped) on each new request so rapid track
    /// switches don't stack downloads.
    prebuffer_cancel: Mutex<Option<std::sync::Arc<std::sync::atomic::AtomicBool>>>,
    /// Holds recent `NamedTempFile` handles so the underlying inode
    /// stays alive while mpv still has it open. Bounded so old files
    /// eventually get unlinked.
    prebuffer_files: Mutex<Vec<std::sync::Arc<tempfile::NamedTempFile>>>,
    /// Per-Buffered-request flag, true between `mpv.stop()` and the
    /// task's `mpv.loadfile`. Suppresses idle-advance during the gap.
    /// Per-task Arc so a stale task's Drop clears only its own flag.
    prebuffer_loading: Mutex<Option<Arc<AtomicBool>>>,
    /// Timestamp of the most recent successful `mpv.loadfile`. mpv may
    /// still report idle-active for a short window after loadfile, so
    /// the idle-advance branch ignores idle within ~1.5s of this.
    last_loadfile: std::sync::Mutex<Option<std::time::Instant>>,
    /// Bumped on every `update_server_config`; library refresh handlers
    /// capture the gen at start and discard their result if it changed,
    /// preventing stale results from one server polluting the next.
    config_gen: std::sync::atomic::AtomicU64,
    /// Flipped to true on shutdown so background spawn tasks (fast
    /// probe, cava watchers) can exit promptly instead of holding
    /// `Arc<Self>` alive until their own timers fire.
    shutdown: std::sync::atomic::AtomicBool,
    /// Wakes futures awaiting shutdown; consumers select on shutdown_signal().
    shutdown_notify: tokio::sync::Notify,
    /// Bumped on each library refresh; LibraryVersionChanged carries it for pull-style clients.
    library_version: std::sync::atomic::AtomicU64,
    /// Throttles repeat preload attempts when network keeps failing; 5s backoff.
    last_preload_attempt: std::sync::Mutex<Option<std::time::Instant>>,
}

impl DaemonCore {
    pub fn new(state: SharedDaemonState, config: &Config) -> Arc<Self> {
        Self::new_with_mpv(state, config, MpvController::new())
    }

    /// Test seam: build a DaemonCore around a pre-built MpvController.
    pub fn new_with_mpv(
        state: SharedDaemonState,
        config: &Config,
        mpv: MpvController,
    ) -> Arc<Self> {
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
            mpv: Mutex::new(mpv),
            pipewire: Mutex::new(PipeWireController::new()),
            subsonic: RwLock::new(subsonic),
            event_tx,
            queue_save_tx,
            cover_art_cache: RwLock::new(crate::daemon::library::LruCache::new()),
            prebuffer_cancel: Mutex::new(None),
            prebuffer_files: Mutex::new(Vec::new()),
            prebuffer_loading: Mutex::new(None),
            last_loadfile: std::sync::Mutex::new(None),
            config_gen: std::sync::atomic::AtomicU64::new(0),
            shutdown: std::sync::atomic::AtomicBool::new(false),
            shutdown_notify: tokio::sync::Notify::new(),
            library_version: std::sync::atomic::AtomicU64::new(0),
            last_preload_attempt: std::sync::Mutex::new(None),
        });

        core.clone().spawn_queue_persistence(queue_save_rx);
        Self::sweep_orphan_prebuffer_files();
        core
    }

    /// Best-effort cleanup of `/tmp/ferrosonic-prebuf-*.dat` left
    /// behind by previous crashes (spawn task panics never run the
    /// NamedTempFile destructor).
    fn sweep_orphan_prebuffer_files() {
        let Ok(entries) = std::fs::read_dir(std::env::temp_dir()) else {
            return;
        };
        // Only files older than 5 min: avoids racing a concurrent
        // ferrosonic instance whose prebuffer task is still alive.
        let cutoff = std::time::Duration::from_secs(300);
        for entry in entries.flatten() {
            let name = entry.file_name();
            let Some(name_str) = name.to_str() else {
                continue;
            };
            if !name_str.starts_with("ferrosonic-prebuf-") || !name_str.ends_with(".dat") {
                continue;
            }
            let path = entry.path();
            let Ok(meta) = std::fs::metadata(&path) else {
                continue;
            };
            let Ok(mtime) = meta.modified() else {
                continue;
            };
            let Ok(age) = std::time::SystemTime::now().duration_since(mtime) else {
                continue;
            };
            if age > cutoff {
                let _ = std::fs::remove_file(&path);
            }
        }
    }

    fn stamp_loadfile(&self) {
        *self.last_loadfile.lock().unwrap() = Some(std::time::Instant::now());
    }

    fn spawn_queue_persistence(
        self: Arc<Self>,
        mut rx: tokio::sync::mpsc::Receiver<()>,
    ) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = self.shutdown_signal() => return,
                    next = rx.recv() => {
                        if next.is_none() { return; }
                    }
                }
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                while rx.try_recv().is_ok() {}
                if self.shutdown.load(std::sync::atomic::Ordering::Acquire) {
                    return;
                }
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

    pub async fn spawn_mpv_event_listener(self: &Arc<Self>) -> tokio::task::JoinHandle<()> {
        let core = self.clone();
        let mut rx = core.mpv.lock().await.subscribe_events();
        tokio::spawn(async move {
            use crate::audio::mpv::MpvEventKind;
            loop {
                if core.shutdown.load(std::sync::atomic::Ordering::Acquire) {
                    return;
                }
                let ev = match rx.recv().await {
                    Ok(e) => e,
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                        warn!("mpv event listener lagged; probing idle state");
                        if let Ok(true) = core.mpv.lock().await.is_idle().await {
                            let _ = core.advance_auto().await;
                        }
                        continue;
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => return,
                };
                if let MpvEventKind::EndFile { reason } = ev {
                    if reason != "eof" {
                        continue;
                    }
                    if core.shutdown.load(std::sync::atomic::Ordering::Acquire) {
                        return;
                    }
                    let count = core
                        .mpv
                        .lock()
                        .await
                        .get_playlist_count()
                        .await
                        .unwrap_or(0);
                    if count >= 2 {
                        debug!("end-file eof during gapless preload; poll owns advance");
                        continue;
                    }
                    debug!("mpv end-file (eof) with no preload; advancing");
                    let _ = core.advance_auto().await;
                }
            }
        })
    }

    pub async fn quit_mpv(&self) {
        self.request_shutdown();
        let mut mpv = self.mpv.lock().await;
        let _ = mpv.quit().await;
    }

    pub fn spawn_polling_task(self: &Arc<Self>) -> tokio::task::JoinHandle<()> {
        let core = self.clone();
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(std::time::Duration::from_millis(500));
            let mut watchdog =
                tokio::time::interval(std::time::Duration::from_secs(2));
            tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            watchdog.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                tokio::select! {
                    _ = core.shutdown_signal() => return,
                    _ = tick.tick() => core.update_playback_info().await,
                    _ = watchdog.tick() => {
                        if core.shutdown.load(std::sync::atomic::Ordering::Acquire) {
                            return;
                        }
                        let dead = !core.mpv.lock().await.is_running();
                        if dead {
                            warn!("mpv backend gone, respawning");
                            if let Err(e) = core.start_mpv().await {
                                error!("respawn failed: {}", e);
                            }
                        }
                    }
                }
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

    pub async fn broadcast_now_playing(&self) {
        self.emit_now_playing().await;
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
        let Some(client) = self.subsonic.read().await.clone() else {
            return;
        };
        let gen_at_start = self.config_gen.load(std::sync::atomic::Ordering::Acquire);
        match client.get_starred_songs().await {
            Ok(songs) => {
                if self.config_gen_changed(gen_at_start) {
                    debug!("refresh_starred: config changed mid-request, discarding");
                    return;
                }
                let mut state = self.state.write().await;
                state.library.starred_songs = songs.clone();
                state.library.rebuild_starred_index();
                drop(state);
                self.emit(DaemonEvent::StarredChanged(songs));
                self.bump_library_version();
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
        let Some(client) = self.subsonic.read().await.clone() else {
            return;
        };
        let gen_at_start = self.config_gen.load(std::sync::atomic::Ordering::Acquire);
        match client.get_random_songs().await {
            Ok(songs) => {
                if self.config_gen_changed(gen_at_start) {
                    debug!("refresh_random: config changed mid-request, discarding");
                    return;
                }
                let mut state = self.state.write().await;
                state.library.random_songs = songs.clone();
                drop(state);
                self.emit(DaemonEvent::RandomChanged(songs));
                self.bump_library_version();
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
        let Some(client) = self.subsonic.read().await.clone() else {
            return;
        };
        let gen_at_start = self.config_gen.load(std::sync::atomic::Ordering::Acquire);
        match client.get_artists().await {
            Ok(artists) => {
                if self.config_gen_changed(gen_at_start) {
                    debug!("refresh_artists: config changed mid-request, discarding");
                    return;
                }
                let mut state = self.state.write().await;
                let count = artists.len();
                state.library.artists = artists.clone();
                drop(state);
                info!("Loaded {} artists", count);
                self.emit(DaemonEvent::ArtistsChanged(artists));
                self.bump_library_version();
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
        let Some(client) = self.subsonic.read().await.clone() else {
            return;
        };
        let gen_at_start = self.config_gen.load(std::sync::atomic::Ordering::Acquire);
        match client.get_playlists().await {
            Ok(playlists) => {
                if self.config_gen_changed(gen_at_start) {
                    debug!("refresh_playlists: config changed mid-request, discarding");
                    return;
                }
                let mut state = self.state.write().await;
                let count = playlists.len();
                state.library.playlists = playlists.clone();
                drop(state);
                info!("Loaded {} playlists", count);
                self.emit(DaemonEvent::PlaylistsChanged(playlists));
                self.bump_library_version();
            }
            Err(e) => {
                error!("Failed to load playlists: {}", e);
            }
        }
    }

    fn config_gen_changed(&self, snapshot: u64) -> bool {
        self.config_gen.load(std::sync::atomic::Ordering::Acquire) != snapshot
    }

    fn bump_library_version(&self) {
        let v = self
            .library_version
            .fetch_add(1, std::sync::atomic::Ordering::Release)
            + 1;
        self.emit(DaemonEvent::LibraryVersionChanged(v));
    }

    pub async fn toggle_star_song(self: &Arc<Self>, song_id: &str) -> Result<bool, Error> {
        let currently_starred = {
            let state = self.state.read().await;
            song_is_starred(&state, song_id)
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
            apply_star_to_cached(&mut state, song_id, new_starred);
        }
        self.emit(DaemonEvent::SongStarChanged {
            id: song_id.to_string(),
            starred: new_starred,
        });
        self.refresh_starred().await;
        Ok(new_starred)
    }

    pub async fn load_artist(self: &Arc<Self>, artist_id: &str) {
        let Some(client) = self.subsonic.read().await.clone() else {
            return;
        };
        match client.get_artist(artist_id).await {
            Ok((_artist, albums)) => {
                let mut state = self.state.write().await;
                let count = albums.len();
                let lib = &mut state.library;
                crate::daemon::library::cache_insert(
                    &mut lib.albums_cache,
                    &mut lib.albums_cache_order,
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

    /// Manual skip. Ignores `repeat=One` (user wants to move). rust-audit: skip
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
            if self.extend_with_random_and_play().await? {
                return Ok(());
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

    /// Fetch random songs, extend queue and play first new track under one write lock so another client cannot mutate the queue between extend and play_from index.
    async fn extend_with_random_and_play(self: &Arc<Self>) -> Result<bool, Error> {
        info!("Queue ended, auto-continuing with random songs");
        let Some(client) = self.subsonic.read().await.clone() else {
            return Ok(false);
        };
        let songs = match client.get_random_songs().await {
            Ok(s) if !s.is_empty() => s,
            Ok(_) => {
                self.emit(DaemonEvent::Notification {
                    message: "Auto-continue: server returned no songs".to_string(),
                    is_error: true,
                });
                return Ok(false);
            }
            Err(e) => {
                error!("Auto-continue fetch failed: {}", e);
                self.emit(DaemonEvent::Notification {
                    message: format!("Auto-continue failed: {}", e),
                    is_error: true,
                });
                return Ok(false);
            }
        };
        let prepared = {
            let mut state = self.state.write().await;
            let start_pos = state.queue.len();
            state.queue.extend(songs);
            self.commit_play_state_in_lock(&mut state, &client, start_pos)
                .ok()
                .map(|(s, u)| (s, u, start_pos))
        };
        self.broadcast_queue_changed().await;
        let Some((song, stream_url, idx)) = prepared else {
            return Ok(false);
        };
        info!("Playing: {} (queue pos {}) mode=Buffered", song.title, idx);
        self.dispatch_play(stream_url, idx, PlayMode::Buffered).await?;
        self.emit_now_playing().await;
        self.emit_queue().await;
        self.spawn_fast_probe();
        Ok(true)
    }

    /// Auto-end advance. Honours `repeat=One` and `repeat=All`. rust-audit: skip
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
            if self.extend_with_random_and_play().await? {
                return Ok(());
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

    pub async fn play_queue_position(
        self: &Arc<Self>,
        pos: usize,
        mode: PlayMode,
    ) -> Result<(), Error> {
        let Some(client) = self.subsonic.read().await.clone() else {
            return Ok(());
        };

        let (song, stream_url) = {
            let mut state = self.state.write().await;
            match self.commit_play_state_in_lock(&mut state, &client, pos) {
                Ok(v) => v,
                Err(_) => return Ok(()),
            }
        };

        info!(
            "Playing: {} (queue pos {}) mode={:?}",
            song.title, pos, mode
        );

        self.dispatch_play(stream_url, pos, mode).await?;
        self.emit_now_playing().await;
        self.emit_queue().await;
        self.spawn_fast_probe();
        Ok(())
    }

    /// Replace queue + play target under a single state write lock so
    /// another client cannot mutate the queue between the swap and the
    /// play setup. If `play_from` is None, only the queue is replaced.
    pub async fn replace_queue_and_play(
        self: &Arc<Self>,
        songs: Vec<crate::subsonic::models::Child>,
        play_from: Option<usize>,
        mode: PlayMode,
    ) -> Result<(), Error> {
        let client_opt = self.subsonic.read().await.clone();

        let prepared = {
            let mut state = self.state.write().await;
            state.queue = songs;
            state.queue_position = None;
            match (play_from, client_opt) {
                (Some(idx), Some(client)) => self
                    .commit_play_state_in_lock(&mut state, &client, idx)
                    .ok()
                    .map(|(s, u)| (s, u, idx)),
                _ => None,
            }
        };

        self.broadcast_queue_changed().await;

        let Some((song, stream_url, idx)) = prepared else {
            return Ok(());
        };

        info!(
            "Playing: {} (queue pos {}) mode={:?}",
            song.title, idx, mode
        );

        self.dispatch_play(stream_url, idx, mode).await?;
        self.emit_now_playing().await;
        self.emit_queue().await;
        self.spawn_fast_probe();
        Ok(())
    }

    /// Validate queue[pos], fetch its stream URL, and commit play
    /// state. Must be called with `state` already write-locked.
    fn commit_play_state_in_lock(
        self: &Arc<Self>,
        state: &mut DaemonState,
        client: &SubsonicClient,
        pos: usize,
    ) -> Result<(crate::subsonic::models::Child, String), ()> {
        use crate::app::state::PlaybackState;
        let song = match state.queue.get(pos) {
            Some(s) => s.clone(),
            None => return Err(()),
        };
        let url = match client.get_stream_url(&song.id) {
            Ok(url) => url,
            Err(e) => {
                error!("Failed to get stream URL: {}", e);
                self.emit(DaemonEvent::Notification {
                    message: format!("Failed to get stream URL: {}", e),
                    is_error: true,
                });
                return Err(());
            }
        };
        state.queue_position = Some(pos);
        state.now_playing.song = Some(song.clone());
        state.now_playing.state = PlaybackState::Playing;
        state.now_playing.position = 0.0;
        state.now_playing.duration = song.duration.unwrap_or(0) as f64;
        state.now_playing.sample_rate = None;
        state.now_playing.bit_depth = None;
        state.now_playing.format = None;
        state.now_playing.channels = None;
        Ok((song, url))
    }

    async fn dispatch_play(
        self: &Arc<Self>,
        stream_url: String,
        pos: usize,
        mode: PlayMode,
    ) -> Result<(), Error> {
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
                self.stamp_loadfile();
                drop(mpv);
                self.preload_next_track(pos).await;
            }
            PlayMode::Buffered => {
                let loading = Arc::new(AtomicBool::new(true));
                let cancel = Arc::new(AtomicBool::new(false));
                {
                    let mut cancel_slot = self.prebuffer_cancel.lock().await;
                    let mut loading_slot = self.prebuffer_loading.lock().await;
                    if let Some(prev) = cancel_slot.replace(cancel.clone()) {
                        prev.store(true, Ordering::Relaxed);
                    }
                    let _ = loading_slot.replace(loading.clone());
                }
                let mut owner = LoadingFlagOwner::new(loading.clone());
                {
                    let mut mpv = self.mpv.lock().await;
                    if mpv.is_paused().await.unwrap_or(false) {
                        let _ = mpv.resume().await;
                    }
                    if mpv.is_running() && !mpv.is_idle().await.unwrap_or(true) {
                        let _ = mpv.stop().await;
                    }
                }
                self.prebuffer_and_load(stream_url, pos, loading, cancel).await;
                owner.disarm();
            }
        }
        Ok(())
    }

    fn spawn_fast_probe(self: &Arc<Self>) {
        // 50ms x 80 = ~4s ceiling for mpv to populate audio params,
        // so the quality row doesn't lag the 500ms backstop tick.
        let core = self.clone();
        tokio::spawn(async move {
            for _ in 0..80 {
                if core.shutdown.load(std::sync::atomic::Ordering::Acquire) {
                    return;
                }
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
    }

    /// Signal background spawn tasks to exit.
    pub fn request_shutdown(&self) {
        self.shutdown
            .store(true, std::sync::atomic::Ordering::Release);
        self.shutdown_notify.notify_waiters();
    }

    /// Resolves immediately if already shut down, else on next request_shutdown.
    pub async fn shutdown_signal(&self) {
        let fut = self.shutdown_notify.notified();
        tokio::pin!(fut);
        if self.shutdown.load(std::sync::atomic::Ordering::Acquire) {
            return;
        }
        fut.await;
    }

    /// Smooth track swap: stream the new URL to a local temp file
    /// while the currently-playing mpv source keeps going. When the
    /// pre-buffer threshold is met (or the download finishes for a
    /// small file), point mpv at the local file via `loadfile`. Old
    /// audio stays continuous up to the loadfile moment; mpv reads
    /// from disk and starts decoding immediately.
    async fn prebuffer_and_load(
        self: &Arc<Self>,
        url: String,
        preload_pos: usize,
        loading: Arc<AtomicBool>,
        cancel: Arc<AtomicBool>,
    ) {
        use std::sync::Arc as StdArc;

        let temp = match tempfile::Builder::new()
            .prefix("ferrosonic-prebuf-")
            .suffix(".dat")
            .tempfile()
        {
            Ok(t) => StdArc::new(t),
            Err(e) => {
                error!(
                    "Pre-buffer: temp file create failed ({}); falling back to direct loadfile",
                    e
                );
                let mut mpv = self.mpv.lock().await;
                let _ = mpv.loadfile(&url).await;
                self.stamp_loadfile();
                loading.store(false, Ordering::Release);
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

            // RAII clears on every return: loading flag + cancel slot.
            let gate = PrebufferGate::new(loading);
            let slot_cleaner = CancelSlotCleaner::new(core.clone(), cancel_task.clone());

            let path = temp_task.path().to_path_buf();
            let path_str = path.to_string_lossy().to_string();
            let start = std::time::Instant::now();

            let client = reqwest::Client::builder()
                .connect_timeout(std::time::Duration::from_secs(10))
                .timeout(std::time::Duration::from_secs(60))
                .build()
                .unwrap_or_else(|_| reqwest::Client::new());
            let resp = match client.get(&url).send().await {
                Ok(r) => r,
                Err(e) => {
                    error!("Pre-buffer fetch failed: {}", e);
                    let mut mpv = core.mpv.lock().await;
                    let _ = mpv.loadfile(&url).await;
                    core.stamp_loadfile();
                    return;
                }
            };

            let mut file = match std::fs::File::create(&path) {
                Ok(f) => f,
                Err(e) => {
                    error!("Pre-buffer file open failed: {}", e);
                    let mut mpv = core.mpv.lock().await;
                    let _ = mpv.loadfile(&url).await;
                    core.stamp_loadfile();
                    return;
                }
            };

            let mut bytes_written: usize = 0;
            let mut triggered = false;
            let mut stream = resp.bytes_stream();

            loop {
                if cancel_task.load(Ordering::Relaxed) {
                    debug!("Pre-buffer cancelled at {} KB", bytes_written / 1024);
                    gate.disarm();
                    slot_cleaner.disarm();
                    return;
                }
                if core.shutdown.load(Ordering::Acquire) {
                    debug!("Pre-buffer exiting on shutdown");
                    return;
                }
                let next = tokio::time::timeout(
                    std::time::Duration::from_secs(15),
                    stream.next(),
                )
                .await;
                let chunk_opt = match next {
                    Ok(c) => c,
                    Err(_) => {
                        error!("Pre-buffer stream timeout (15s); aborting");
                        if !triggered {
                            let mut mpv = core.mpv.lock().await;
                            let _ = mpv.loadfile(&url).await;
                            core.stamp_loadfile();
                        }
                        return;
                    }
                };
                let Some(chunk) = chunk_opt else { break };
                let chunk = match chunk {
                    Ok(c) => c,
                    Err(e) => {
                        error!("Pre-buffer stream error: {}", e);
                        if !triggered {
                            let mut mpv = core.mpv.lock().await;
                            let _ = mpv.loadfile(&url).await;
                            core.stamp_loadfile();
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
                        // Re-check cancel with the mpv lock held; a newer
                        // switch may have set it between the loop check
                        // and now.
                        if cancel_task.load(Ordering::Relaxed) {
                            debug!("Pre-buffer cancelled before loadfile");
                            gate.disarm();
                            slot_cleaner.disarm();
                            return;
                        }
                        if let Err(e) = mpv.loadfile(&path_str).await {
                            error!("Pre-buffer loadfile failed: {}", e);
                            return;
                        }
                        core.stamp_loadfile();
                    }
                    if cancel_task.load(Ordering::Relaxed) {
                        gate.disarm();
                        slot_cleaner.disarm();
                        return;
                    }
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
                        gate.disarm();
                        slot_cleaner.disarm();
                        return;
                    }
                    if let Err(e) = mpv.loadfile(&path_str).await {
                        error!("Pre-buffer loadfile failed: {}", e);
                        return;
                    }
                    core.stamp_loadfile();
                }
                if cancel_task.load(Ordering::Relaxed) {
                    gate.disarm();
                    slot_cleaner.disarm();
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
            let _ = &slot_cleaner;
        });
    }

    /// Repeat-aware: loads current for One, wraps for All, no-ops
    /// at the end for Off.
    pub async fn preload_next_track(self: &Arc<Self>, current_pos: usize) {
        let next_song = {
            let state = self.state.read().await;
            let queue_len = state.queue.len();
            let target = state.config.repeat_mode.next_auto(current_pos, queue_len);
            match target.and_then(|p| state.queue.get(p)) {
                Some(s) => s.clone(),
                None => return,
            }
        };

        let url = {
            let Some(client) = self.subsonic.read().await.clone() else {
                return;
            };
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
                warn!(
                    "Preload may have failed: playlist count is {} (expected 2)",
                    count
                );
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
        {
            // Single pw lock spans the set, and we always call set_rate
            // (no cache short-circuit) so external pw-metadata changes
            // don't leave us silently mismatched.
            let mut pw = self.pipewire.lock().await;
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
    /// rust-audit: skip (guard-reads + self-contained gapless write block by design)
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
            let (time_remaining, has_next, position) = {
                let state = self.state.read().await;
                let tr = state.now_playing.duration - state.now_playing.position;
                let hn = state
                    .queue_position
                    .map(|p| p + 1 < state.queue.len())
                    .unwrap_or(false);
                (tr, hn, state.now_playing.position)
            };

            // position > 0.5 guards against a freshly loaded track
            // whose `time_remaining` lands in (0, 2) because state was
            // pre-populated before mpv actually started decoding.
            if has_next && position > 0.5 && time_remaining > 0.0 && time_remaining < 2.0 {
                let count_opt = {
                    let mut mpv = self.mpv.lock().await;
                    mpv.get_playlist_count().await.ok()
                };
                if let Some(count) = count_opt {
                    if count < 2 {
                        info!("Near end of track with no preloaded next, advancing early");
                        let _ = self.advance_auto().await;
                        return;
                    }
                }
            }

            let count_opt = {
                let mut mpv = self.mpv.lock().await;
                mpv.get_playlist_count().await.ok()
            };
            if count_opt == Some(1) {
                let should_try = {
                    let mut last = self.last_preload_attempt.lock().unwrap();
                    let due = last
                        .map(|t| t.elapsed() >= std::time::Duration::from_secs(5))
                        .unwrap_or(true);
                    if due {
                        *last = Some(std::time::Instant::now());
                    }
                    due
                };
                if should_try {
                    let cur_pos_opt = {
                        let state = self.state.read().await;
                        state.queue_position
                    };
                    if let Some(pos) = cur_pos_opt {
                        debug!("Playlist count is 1, re-preloading next track");
                        self.preload_next_track(pos).await;
                    }
                }
            }

            let mpv_pos_opt = {
                let mut mpv = self.mpv.lock().await;
                mpv.get_playlist_pos().await.ok().flatten()
            };
            if mpv_pos_opt == Some(1) {
                // Resolve next-song and commit state under a single
                // write lock; a queue mutation between resolution and
                // commit would desync now_playing.
                let next_pos = {
                    let mut state = self.state.write().await;
                    let queue_len = state.queue.len();
                    let repeat = state.config.repeat_mode;
                    let resolved = state.queue_position.and_then(|cur| {
                        repeat
                            .next_auto(cur, queue_len)
                            .and_then(|n| state.queue.get(n).map(|s| (n, s.clone())))
                    });
                    if let Some((next_pos, song)) = resolved {
                        state.queue_position = Some(next_pos);
                        state.now_playing.song = Some(song.clone());
                        state.now_playing.position = 0.0;
                        state.now_playing.duration = song.duration.unwrap_or(0) as f64;
                        Some(next_pos)
                    } else {
                        None
                    }
                };
                if let Some(next_pos) = next_pos {
                    info!("Gapless advancement to track {}", next_pos);
                    {
                        // Re-check playlist-pos under the same mpv lock
                        // as the remove so we don't pop entry 0 after
                        // mpv has moved on or rewound.
                        let mut mpv = self.mpv.lock().await;
                        let pos_now = mpv.get_playlist_pos().await.ok().flatten();
                        if pos_now == Some(1) {
                            let _ = mpv.playlist_remove(0).await;
                        } else {
                            warn!(
                                "playlist-pos shifted from 1 to {:?} before remove; skipping",
                                pos_now
                            );
                        }
                    }
                    self.preload_next_track(next_pos).await;
                    self.emit_now_playing().await;
                    // queue_position changed; clients derive current_song from it.
                    self.emit_queue().await;
                    return;
                }
            }

            let idle_opt = {
                let mut mpv = self.mpv.lock().await;
                mpv.is_idle().await.ok()
            };
            if idle_opt == Some(true) {
                // Buffered switch stopped mpv mid-download.
                let loading = self
                    .prebuffer_loading
                    .lock()
                    .await
                    .as_ref()
                    .map(|a| a.load(Ordering::Acquire))
                    .unwrap_or(false);
                if loading {
                    debug!("mpv idle but prebuffer in flight; deferring advance");
                    return;
                }
                // Post-loadfile acceptance window: mpv may still report
                // idle for a brief moment after a successful loadfile.
                let just_loaded = self
                    .last_loadfile
                    .lock()
                    .unwrap()
                    .map(|t| t.elapsed() < std::time::Duration::from_millis(1500))
                    .unwrap_or(false);
                if just_loaded {
                    debug!("mpv idle but loadfile <1.5s ago; deferring advance");
                    return;
                }
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
    pub async fn load_album_songs(
        self: &Arc<Self>,
        album_id: &str,
    ) -> Vec<crate::subsonic::models::Child> {
        let Some(client) = self.subsonic.read().await.clone() else {
            return Vec::new();
        };
        match client.get_album(album_id).await {
            Ok((_album, songs)) => {
                {
                    let mut state = self.state.write().await;
                    let lib = &mut state.library;
                    crate::daemon::library::cache_insert(
                        &mut lib.album_songs_cache,
                        &mut lib.album_songs_cache_order,
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

    pub async fn load_playlist_songs(
        self: &Arc<Self>,
        playlist_id: &str,
    ) -> Vec<crate::subsonic::models::Child> {
        let Some(client) = self.subsonic.read().await.clone() else {
            return Vec::new();
        };
        match client.get_playlist(playlist_id).await {
            Ok((_pl, songs)) => {
                {
                    let mut state = self.state.write().await;
                    let lib = &mut state.library;
                    crate::daemon::library::cache_insert(
                        &mut lib.playlist_songs_cache,
                        &mut lib.playlist_songs_cache_order,
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
            // When password_file is set, secret goes to that file and
            // inline stays empty in config.toml; otherwise inline.
            let pf_opt = state.config.password_file.clone().filter(|s| !s.is_empty());
            if let Some(pf) = pf_opt.as_deref() {
                if let Err(e) = crate::config::write_password_file_atomic(pf, password) {
                    error!("Failed to write password to {}: {}", pf, e);
                    return Err(Error::Io(e));
                }
                state.config.password = String::new();
                state.config.save_default().map_err(Error::Config)?;
                state.config.password = password.to_string();
            } else {
                state.config.password = password.to_string();
                state.config.save_default().map_err(Error::Config)?;
            }
        }

        let new_client =
            SubsonicClient::new(base_url, username, password).map_err(Error::Subsonic)?;
        *self.subsonic.write().await = Some(new_client);
        // Bump after the new client is installed so in-flight refreshes
        // started with the old client discard their results.
        self.config_gen
            .fetch_add(1, std::sync::atomic::Ordering::Release);

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
            state.config.save_default().map_err(Error::Config)?;
        }
        self.emit_config_changed().await;
        Ok(())
    }

    pub async fn set_cava_enabled(self: &Arc<Self>, on: bool) -> Result<(), Error> {
        {
            let mut state = self.state.write().await;
            state.config.cava = on;
            state.config.save_default().map_err(Error::Config)?;
        }
        self.emit_config_changed().await;
        Ok(())
    }

    /// Takes effect on the next TUI launch.
    pub async fn set_daemon_enabled(self: &Arc<Self>, on: bool) -> Result<(), Error> {
        {
            let mut state = self.state.write().await;
            state.config.daemon = on;
            state.config.save_default().map_err(Error::Config)?;
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
    pub async fn set_repeat_mode(
        self: &Arc<Self>,
        mode: crate::config::RepeatMode,
    ) -> Result<(), Error> {
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
            let mut cache = self.cover_art_cache.write().await;
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
                cache.insert(
                    key,
                    bytes.clone(),
                    crate::daemon::library::COVER_ART_CACHE_CAP,
                );
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
            state.config.save_default().map_err(Error::Config)?;
        }
        self.emit_config_changed().await;
        Ok(())
    }
}

fn song_is_starred(daemon: &DaemonState, song_id: &str) -> bool {
    if daemon.library.starred_ids.contains(song_id) {
        return true;
    }
    // Fallback to a full scan when the per-song `starred` marker
    // lives only in a different cache (queue, random, album_songs,
    // playlist_songs) or tests mutated starred_songs directly.
    daemon
        .library
        .starred_songs
        .iter()
        .chain(daemon.queue.iter())
        .chain(daemon.library.random_songs.iter())
        .chain(daemon.library.album_songs_cache.values().flatten())
        .chain(daemon.library.playlist_songs_cache.values().flatten())
        .any(|s| s.id == song_id && s.starred.is_some())
}

fn apply_star_to_cached(daemon: &mut DaemonState, song_id: &str, starred: bool) {
    let marker = if starred { Some("1".to_string()) } else { None };
    let lists: [&mut Vec<crate::subsonic::models::Child>; 2] =
        [&mut daemon.queue, &mut daemon.library.random_songs];
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
            np.starred = marker.clone();
        }
    }
    sync_starred_songs(daemon, song_id, starred, marker);
}

fn sync_starred_songs(
    daemon: &mut DaemonState,
    song_id: &str,
    starred: bool,
    marker: Option<String>,
) {
    if starred {
        daemon.library.starred_ids.insert(song_id.to_string());
        let already = daemon
            .library
            .starred_songs
            .iter()
            .any(|s| s.id == song_id);
        if !already {
            let source = daemon
                .queue
                .iter()
                .chain(daemon.library.random_songs.iter())
                .chain(daemon.library.album_songs_cache.values().flatten())
                .chain(daemon.library.playlist_songs_cache.values().flatten())
                .find(|s| s.id == song_id)
                .cloned();
            if let Some(mut s) = source {
                s.starred = marker;
                daemon.library.starred_songs.push(s);
            }
        } else {
            for s in daemon.library.starred_songs.iter_mut() {
                if s.id == song_id {
                    s.starred = marker.clone();
                }
            }
        }
    } else {
        daemon.library.starred_ids.remove(song_id);
        daemon.library.starred_songs.retain(|s| s.id != song_id);
    }
}
