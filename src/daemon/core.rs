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
    pub(super) cover_art_cache: RwLock<crate::daemon::library::LruCache<Vec<u8>>>,
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
    pub(super) prebuffer_loading: Mutex<Option<Arc<AtomicBool>>>,
    /// Timestamp of the most recent successful `mpv.loadfile`. mpv may
    /// still report idle-active for a short window after loadfile, so
    /// the idle-advance branch ignores idle within ~1.5s of this.
    pub(super) last_loadfile: std::sync::Mutex<Option<std::time::Instant>>,
    /// Bumped on every `update_server_config`; library refresh handlers
    /// capture the gen at start and discard their result if it changed,
    /// preventing stale results from one server polluting the next.
    pub(super) config_gen: std::sync::atomic::AtomicU64,
    /// Flipped to true on shutdown so background spawn tasks (fast
    /// probe, cava watchers) can exit promptly instead of holding
    /// `Arc<Self>` alive until their own timers fire.
    shutdown: std::sync::atomic::AtomicBool,
    /// Wakes futures awaiting shutdown; consumers select on shutdown_signal().
    shutdown_notify: tokio::sync::Notify,
    /// Bumped on each library refresh; LibraryVersionChanged carries it for pull-style clients.
    library_version: std::sync::atomic::AtomicU64,
    /// Throttles repeat preload attempts when network keeps failing; 5s backoff.
    pub(super) last_preload_attempt: std::sync::Mutex<Option<std::time::Instant>>,
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
        let mut guard = self
            .last_loadfile
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *guard = Some(std::time::Instant::now());
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

    pub fn subscribe(&self) -> broadcast::Receiver<DaemonEvent> {
        self.event_tx.subscribe()
    }

    pub(super) fn emit(&self, event: DaemonEvent) {
        let _ = self.event_tx.send(event);
    }

    pub async fn broadcast_now_playing(&self) {
        self.emit_now_playing().await;
    }

    pub(super) async fn emit_now_playing(&self) {
        let np = {
            let state = self.state.read().await;
            state.now_playing.clone()
        };
        self.emit(DaemonEvent::NowPlayingChanged(np));
    }

    pub(super) async fn emit_queue(&self) {
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
        // Mask the wire path explicitly. Secret::Serialize would emit "***" but we want "" so the client treats it as empty.
        cfg.password.clear();
        cfg.password_file = None;
        self.emit(DaemonEvent::ConfigChanged(cfg));
    }
}

impl DaemonCore {
    pub(super) fn config_gen_changed(&self, snapshot: u64) -> bool {
        self.config_gen.load(std::sync::atomic::Ordering::Acquire) != snapshot
    }

    #[doc(hidden)]
    pub fn config_gen_for_test(&self) -> u64 {
        self.config_gen.load(std::sync::atomic::Ordering::Acquire)
    }

    pub(super) fn bump_library_version(&self) {
        let v = self
            .library_version
            .fetch_add(1, std::sync::atomic::Ordering::Release)
            + 1;
        self.emit(DaemonEvent::LibraryVersionChanged(v));
    }
}

impl DaemonCore {
    /// Fetch random songs, extend queue and play first new track under one write lock so another client cannot mutate the queue between extend and play_from index.
    pub(super) async fn extend_with_random_and_play(self: &Arc<Self>) -> Result<bool, Error> {
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

    /// Validate queue[pos], fetch its stream URL, and commit play
    /// state. Must be called with `state` already write-locked.
    pub(super) fn commit_play_state_in_lock(
        self: &Arc<Self>,
        state: &mut DaemonState,
        client: &SubsonicClient,
        pos: usize,
    ) -> Result<(crate::subsonic::models::Child, String), ()> {
        use crate::daemon::state::PlaybackState;
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
        // R2: stamp last_loadfile under the state write lock so the 1.5s idle-advance gate in update_playback_info covers the in-flight loadfile, not only the post-loadfile window.
        self.stamp_loadfile();
        Ok((song, url))
    }

    pub(super) async fn dispatch_play(
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

    pub(super) fn spawn_fast_probe(self: &Arc<Self>) {
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

    /// Query mpv for sample rate / bit depth / format / channels and,
    /// if available, write them into state, drive the PipeWire rate
    /// switch, and emit `NowPlayingChanged`. Returns `true` when audio
    /// properties were populated this call.
    pub(super) async fn fetch_audio_properties(self: &Arc<Self>) -> bool {
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

}

impl DaemonCore {
    pub async fn update_server_config(
        self: &Arc<Self>,
        base_url: &str,
        username: &str,
        password: &crate::secret::Secret,
    ) -> Result<(), Error> {
        {
            let mut state = self.state.write().await;
            state.config.base_url = base_url.to_string();
            state.config.username = username.to_string();
            let pf_opt = state.config.password_file.clone().filter(|s| !s.is_empty());
            if let Some(pf) = pf_opt.as_deref() {
                if let Err(e) = crate::config::write_password_file_atomic(pf, password) {
                    error!("Failed to write password to {}: {}", pf, e);
                    return Err(Error::Io(e));
                }
                state.config.password = crate::secret::Secret::new();
                state.config.save_default().map_err(Error::Config)?;
                state.config.password = password.clone();
            } else {
                state.config.password = password.clone();
                state.config.save_default().map_err(Error::Config)?;
            }
        }

        let new_client =
            SubsonicClient::new(base_url, username, password).map_err(Error::Subsonic)?;
        {
            // R4: bump gen before installing client, both under subsonic write so refreshes serialize.
            let mut slot = self.subsonic.write().await;
            self.config_gen
                .fetch_add(1, std::sync::atomic::Ordering::Release);
            slot.replace(new_client);
        }

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
        password: &crate::secret::Secret,
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

