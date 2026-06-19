//! Daemon core: owns mpv, queue, library cache, event broadcast, config persistence. Lock order: state then subsonic then mpv then pipewire then prebuffer_cancel then prebuffer_loading then prebuffer_files then last_loadfile then last_preload_attempt then cover_art_cache. Authoritative table: docs/LOCK-ORDER.md.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use tokio::sync::{broadcast, Mutex, RwLock};
use tracing::{debug, error, info, warn};

use crate::app::state::SharedDaemonState;
use crate::audio::mpv::MpvController;
use crate::audio::pipewire::PipeWireController;
use crate::config::Config;
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

/// RAII counter for a connected IPC client; decrements `active_clients` on drop.
pub struct ClientGuard {
    core: Arc<DaemonCore>,
}

impl Drop for ClientGuard {
    fn drop(&mut self) {
        self.core.active_clients.fetch_sub(1, Ordering::Release);
    }
}

/// Heart of the daemon: owns mpv, `PipeWire`, the Subsonic client, and state.
pub struct DaemonCore {
    /// Shared daemon state mirror.
    pub state: SharedDaemonState,
    /// mpv process and IPC controller.
    pub mpv: Mutex<MpvController>,
    /// `PipeWire` sample-rate controller.
    pub pipewire: Mutex<PipeWireController>,
    /// Subsonic client; `None` until the server is configured.
    pub subsonic: RwLock<Option<SubsonicClient>>,
    /// Broadcast channel feeding `DaemonEvent`s to subscribers.
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
    /// Bumped by `stamp_loadfile` on every loadfile. A spawned rate-settle
    /// captures the gen at its load and refuses to unpause if a newer load
    /// has superseded it, so a rapid track switch can't start the wrong
    /// track at the previous track's pinned rate.
    pub(super) loadfile_gen: std::sync::atomic::AtomicU64,
    /// Bumped on every `update_server_config`; library refresh handlers
    /// capture the gen at start and discard their result if it changed,
    /// preventing stale results from one server polluting the next.
    pub(super) config_gen: std::sync::atomic::AtomicU64,
    /// Flipped to true on shutdown so background spawn tasks (fast
    /// probe, cava watchers) can exit promptly instead of holding
    /// `Arc<Self>` alive until their own timers fire.
    pub(super) shutdown: std::sync::atomic::AtomicBool,
    /// Wakes futures awaiting shutdown; consumers select on shutdown_signal().
    shutdown_notify: tokio::sync::Notify,
    /// Bumped on each library refresh; LibraryVersionChanged carries it for pull-style clients.
    library_version: std::sync::atomic::AtomicU64,
    /// Throttles repeat preload attempts when network keeps failing; 5s backoff.
    pub(super) last_preload_attempt: std::sync::Mutex<Option<std::time::Instant>>,
    /// Count of connected IPC clients; the idle-exit monitor shuts the daemon
    /// down once this is 0 and playback is Stopped, so a daemon never orphans.
    pub(super) active_clients: std::sync::atomic::AtomicUsize,
    /// Per-play scrobble tracking; mutated only by the scrobble tick.
    pub(super) scrobble_state: Mutex<crate::daemon::scrobble::ScrobbleState>,
    /// True when the server advertises the `playbackReport` extension.
    pub(super) playback_report_supported: AtomicBool,
    /// Sends desktop notifications on track change (Linux D-Bus); no-op when
    /// disabled in config or when no session bus is reachable.
    pub(super) notifier: crate::daemon::notify::Notifier,
}

impl DaemonCore {
    /// Build the core with a production mpv controller.
    pub fn new(state: SharedDaemonState, config: &Config) -> Arc<Self> {
        Self::new_with_mpv(state, config, MpvController::new())
    }

    /// Test seam: build a DaemonCore around a pre-built MpvController.
    pub fn new_with_mpv(
        state: SharedDaemonState,
        config: &Config,
        mpv: MpvController,
    ) -> Arc<Self> {
        Self::new_with_mpv_and_pipewire(state, config, mpv, PipeWireController::new())
    }

    /// Test seam: build a DaemonCore around pre-built mpv + `PipeWire` controllers, so tests can inject a recording `pw-metadata` runner and assert the force-rate pin is set on play and cleared on pause/stop.
    pub fn new_with_mpv_and_pipewire(
        state: SharedDaemonState,
        config: &Config,
        mpv: MpvController,
        pipewire: PipeWireController,
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
            pipewire: Mutex::new(pipewire),
            subsonic: RwLock::new(subsonic),
            event_tx,
            queue_save_tx,
            cover_art_cache: RwLock::new(crate::daemon::library::LruCache::new()),
            prebuffer_cancel: Mutex::new(None),
            prebuffer_files: Mutex::new(Vec::new()),
            prebuffer_loading: Mutex::new(None),
            last_loadfile: std::sync::Mutex::new(None),
            loadfile_gen: std::sync::atomic::AtomicU64::new(0),
            config_gen: std::sync::atomic::AtomicU64::new(0),
            shutdown: std::sync::atomic::AtomicBool::new(false),
            shutdown_notify: tokio::sync::Notify::new(),
            library_version: std::sync::atomic::AtomicU64::new(0),
            last_preload_attempt: std::sync::Mutex::new(None),
            active_clients: std::sync::atomic::AtomicUsize::new(0),
            scrobble_state: Mutex::new(crate::daemon::scrobble::ScrobbleState::default()),
            playback_report_supported: AtomicBool::new(false),
            notifier: crate::daemon::notify::Notifier::new(),
        });

        core.clone().spawn_queue_persistence(queue_save_rx);
        core.spawn_refresh_scrobble_capability();
        Self::sweep_orphan_prebuffer_files();
        core
    }

    /// Best-effort cleanup of `/tmp/ferrosonic-prebuf-*.dat` left
    /// behind by previous crashes (spawn task panics never run the
    /// NamedTempFile destructor).
    fn sweep_orphan_prebuffer_files() {
        // Older than 5 min: avoids racing a live instance's prebuffer task.
        crate::io_util::sweep_stale_tmp_files(
            "ferrosonic-prebuf-",
            ".dat",
            std::time::Duration::from_secs(300),
        );
    }

    /// Mark a fresh loadfile and return its generation; a spawned settle
    /// passes this back to `settle_rate_then_unpause` to detect supersession.
    fn stamp_loadfile(&self) -> u64 {
        let mut guard = self
            .last_loadfile
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *guard = Some(std::time::Instant::now());
        self.loadfile_gen.fetch_add(1, Ordering::Release) + 1
    }

    /// Idempotent — no-ops if mpv is already running.
    pub async fn start_mpv(&self) -> Result<(), Error> {
        let mut mpv = self.mpv.lock().await;
        mpv.start().await.map_err(Into::into)
    }

    /// Spawn the task that converts mpv end-file events into auto-advance.
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

    /// Flag shutdown and terminate the mpv process.
    pub async fn quit_mpv(&self) {
        self.request_shutdown();
        let mut mpv = self.mpv.lock().await;
        let _ = mpv.quit().await;
    }

    /// Subscribe to the daemon's event broadcast.
    pub fn subscribe(&self) -> broadcast::Receiver<DaemonEvent> {
        self.event_tx.subscribe()
    }

    pub(super) fn emit(&self, event: DaemonEvent) {
        let _ = self.event_tx.send(event);
    }

    /// Push the current now-playing state to all subscribers.
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

    pub(super) async fn emit_config_changed(&self) {
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
    /// Up to `LOOKAHEAD` random songs whose ids are not already in the queue,
    /// so auto-continue never replays a track until the library is exhausted.
    /// When every candidate is already queued the bag is spent, and the raw
    /// batch is returned so playback can continue with repeats.
    async fn pick_unplayed_random(
        self: &Arc<Self>,
        client: &SubsonicClient,
    ) -> Result<Vec<crate::subsonic::models::Child>, crate::error::SubsonicError> {
        const ATTEMPTS: u32 = 3;
        const LOOKAHEAD: usize = 20;
        let mut seen: std::collections::HashSet<String> = self
            .state
            .read()
            .await
            .queue
            .iter()
            .map(|s| s.id.clone())
            .collect();
        let mut fresh = Vec::new();
        let mut fallback = Vec::new();
        for _ in 0..ATTEMPTS {
            let batch = client.get_random_songs().await?;
            if batch.is_empty() {
                break;
            }
            if fallback.is_empty() {
                fallback = batch.clone();
            }
            for song in batch {
                if fresh.len() >= LOOKAHEAD {
                    break;
                }
                if seen.insert(song.id.clone()) {
                    fresh.push(song);
                }
            }
            if !fresh.is_empty() {
                break;
            }
        }
        if fresh.is_empty() {
            fallback.truncate(LOOKAHEAD);
            Ok(fallback)
        } else {
            Ok(fresh)
        }
    }

    pub(super) async fn extend_with_random_and_play(self: &Arc<Self>) -> Result<bool, Error> {
        info!("Queue ended, auto-continuing with random songs");
        let Some(client) = self.subsonic.read().await.clone() else {
            return Ok(false);
        };
        let songs = match self.pick_unplayed_random(&client).await {
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
        self.dispatch_play(stream_url, idx, PlayMode::Buffered, 0.0)
            .await?;
        self.emit_now_playing().await;
        self.emit_queue().await;
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
        start_at: f64,
    ) -> Result<(), Error> {
        match mode {
            PlayMode::Direct => {
                let gen = {
                    let mut mpv = self.mpv.lock().await;
                    let load = if start_at > 0.0 {
                        mpv.loadfile_at_paused(&stream_url, start_at).await
                    } else {
                        mpv.loadfile_paused(&stream_url).await
                    };
                    if let Err(e) = load {
                        error!("Failed to play: {}", e);
                        drop(mpv);
                        self.emit(DaemonEvent::Notification {
                            message: format!("MPV error: {}", e),
                            is_error: true,
                        });
                        return Ok(());
                    }
                    self.stamp_loadfile()
                };
                // Spawn the probe/re-clock/unpause so the IPC caller is not
                // blocked by the settle; the gen guard drops it if superseded.
                let core = self.clone();
                tokio::spawn(async move { core.settle_rate_then_unpause(gen).await });
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
                self.prebuffer_and_load(stream_url, pos, loading, cancel)
                    .await;
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

    /// Register a connected IPC client; the returned guard decrements the count on drop.
    pub fn client_guard(self: &Arc<Self>) -> ClientGuard {
        self.active_clients.fetch_add(1, Ordering::Release);
        ClientGuard { core: self.clone() }
    }

    /// Shut the daemon down once it has been idle (no clients connected and
    /// playback Stopped) for the grace period, so a daemon spawned for a TUI
    /// that has gone away never stays orphaned. Playing or Paused keeps it
    /// alive so audio continues after the TUI closes.
    pub fn spawn_idle_exit_monitor(self: &Arc<Self>) -> tokio::task::JoinHandle<()> {
        const CHECK: std::time::Duration = std::time::Duration::from_secs(15);
        // Counted in CHECK ticks (not wall-clock) so the loop is driveable under
        // tokio's paused-time tests; 2 * 15s = 30s of continuous idle.
        const IDLE_TICKS_TO_EXIT: u32 = 2;
        let core = self.clone();
        tokio::spawn(async move {
            let mut idle_ticks: u32 = 0;
            loop {
                tokio::select! {
                    () = core.shutdown_signal() => return,
                    () = tokio::time::sleep(CHECK) => {}
                }
                if core.shutdown.load(Ordering::Acquire) {
                    return;
                }
                if core.is_idle_for_exit().await {
                    idle_ticks += 1;
                    if idle_ticks >= IDLE_TICKS_TO_EXIT {
                        info!("Daemon idle (no clients, stopped) past grace; exiting");
                        core.request_shutdown();
                        return;
                    }
                } else {
                    idle_ticks = 0;
                }
            }
        })
    }

    /// True when no IPC client is connected and playback is Stopped: the daemon
    /// has no reason to stay running. Playing/Paused or any client keeps it up.
    pub async fn is_idle_for_exit(&self) -> bool {
        use crate::daemon::state::PlaybackState;
        if self.active_clients.load(Ordering::Acquire) != 0 {
            return false;
        }
        self.state.read().await.now_playing.state == PlaybackState::Stopped
    }

    /// Download the new URL to a local temp file in full, then load it paused
    /// and run the rate-switch pre-roll. The whole file is fetched first so mpv
    /// reads the true track length; loading a still-growing file paused makes
    /// mpv treat the partial-file EOF as the track end and advance early.
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
                let next =
                    tokio::time::timeout(std::time::Duration::from_secs(15), stream.next()).await;
                let chunk_opt = match next {
                    Ok(c) => c,
                    Err(_) => {
                        error!("Pre-buffer stream timeout (15s); aborting");
                        let mut mpv = core.mpv.lock().await;
                        let _ = mpv.loadfile(&url).await;
                        core.stamp_loadfile();
                        return;
                    }
                };
                let Some(chunk) = chunk_opt else { break };
                let chunk = match chunk {
                    Ok(c) => c,
                    Err(e) => {
                        error!("Pre-buffer stream error: {}", e);
                        let mut mpv = core.mpv.lock().await;
                        let _ = mpv.loadfile(&url).await;
                        core.stamp_loadfile();
                        return;
                    }
                };
                if let Err(e) = file.write_all(&chunk) {
                    error!("Pre-buffer write error: {}", e);
                    return;
                }
                bytes_written += chunk.len();
            }

            let _ = file.flush();
            info!(
                "Pre-buffer download complete ({} KB in {:?}); loading",
                bytes_written / 1024,
                start.elapsed()
            );
            let gen = {
                let mut mpv = core.mpv.lock().await;
                if cancel_task.load(Ordering::Relaxed) {
                    debug!("Pre-buffer cancelled before loadfile");
                    gate.disarm();
                    slot_cleaner.disarm();
                    return;
                }
                if let Err(e) = mpv.loadfile_paused(&path_str).await {
                    error!("Pre-buffer loadfile failed: {}", e);
                    return;
                }
                core.stamp_loadfile()
            };
            if cancel_task.load(Ordering::Relaxed) {
                gate.disarm();
                slot_cleaner.disarm();
                return;
            }
            core.settle_rate_then_unpause(gen).await;
            core.preload_next_track(preload_pos).await;
            let _ = &slot_cleaner;
        });
    }

    /// Query mpv for sample rate / bit depth / format / channels and,
    /// if available, write them into state, drive the `PipeWire` rate
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

    /// Probe the decoded rate after a paused load, re-clock the `PipeWire`
    /// graph during the paused silence, then unpause. A rate change settles
    /// for `rate_switch_delay_ms` so the device re-lock lands in the pre-roll
    /// gap and not in the first frames of music; same-rate tracks unpause
    /// immediately. Writes the audio props and emits `NowPlayingChanged`.
    /// `gen` is the loadfile generation from the paused load this settles.
    /// Invariant: the caller loaded the track paused; this fn starts it. Bails
    /// at each step if a newer load has superseded `gen`, so it never unpauses
    /// or re-clocks for a track that is no longer current.
    pub(super) async fn settle_rate_then_unpause(self: &Arc<Self>, gen: u64) {
        if self.settle_superseded(gen) {
            return;
        }
        let settle = if let Some((rate, bd, fmt, ch)) = self.probe_audio_params(gen).await {
            if self.settle_superseded(gen) {
                return;
            }
            let changed = {
                let mut pw = self.pipewire.lock().await;
                // Re-check under the pw lock: a newer load taken between the
                // probe and here must not re-clock for a superseded track.
                if self.settle_superseded(gen) {
                    return;
                }
                let changed = pw.get_current_rate() != Some(rate);
                // Always re-issue (staleness defense vs external pw-metadata);
                // only the settle delay is gated on an actual rate change.
                if let Err(e) = pw.set_rate(rate).await {
                    warn!("Failed to set PipeWire sample rate: {}", e);
                }
                changed
            };
            let mut state = self.state.write().await;
            state.now_playing.sample_rate = Some(rate);
            state.now_playing.bit_depth = bd;
            state.now_playing.format = fmt;
            state.now_playing.channels = ch;
            changed.then(|| {
                std::time::Duration::from_millis(u64::from(state.config.rate_switch_delay_ms))
            })
        } else {
            None
        };
        if let Some(settle) = settle {
            tokio::time::sleep(settle).await;
        }
        if self.settle_superseded(gen) {
            return;
        }
        {
            let mut mpv = self.mpv.lock().await;
            if let Err(e) = mpv.resume().await {
                warn!("Failed to unpause after rate settle: {}", e);
            }
        }
        self.emit_now_playing().await;
    }

    /// True when a rate-settle for load `gen` should abandon: shutting down,
    /// or a newer loadfile has bumped the generation past `gen`.
    fn settle_superseded(&self, gen: u64) -> bool {
        self.shutdown.load(Ordering::Acquire) || self.loadfile_gen.load(Ordering::Acquire) != gen
    }

    /// Poll mpv for decoded audio params after a paused load until the sample
    /// rate populates, bounded so a stream that never reports still unblocks
    /// playback (the 500ms tick re-pins it). Bails early if load `gen` is
    /// superseded. Returns `(rate, bit_depth, format, channels)` once known.
    async fn probe_audio_params(
        self: &Arc<Self>,
        gen: u64,
    ) -> Option<(u32, Option<u32>, Option<String>, Option<String>)> {
        const PROBE_INTERVAL: std::time::Duration = std::time::Duration::from_millis(30);
        const PROBE_MAX_ITERS: u32 = 50;
        for i in 0..PROBE_MAX_ITERS {
            if self.settle_superseded(gen) {
                return None;
            }
            let (sr, bd, fmt, ch) = {
                let mut mpv = self.mpv.lock().await;
                (
                    mpv.get_sample_rate().await.ok().flatten(),
                    mpv.get_bit_depth().await.ok().flatten(),
                    mpv.get_audio_format().await.ok().flatten(),
                    mpv.get_channels().await.ok().flatten(),
                )
            };
            if let Some(rate) = sr {
                return Some((rate, bd, fmt, ch));
            }
            if i + 1 < PROBE_MAX_ITERS {
                tokio::time::sleep(PROBE_INTERVAL).await;
            }
        }
        None
    }

    /// Drop the `PipeWire` force-rate pin so the graph follows live streams again; call when playback leaves `Playing` (pause/stop) so an idle daemon stops holding the device at the track's rate.
    pub(super) async fn release_pipewire_rate(self: &Arc<Self>) {
        let mut pw = self.pipewire.lock().await;
        if let Err(e) = pw.clear_forced_rate().await {
            warn!("Failed to clear PipeWire forced rate: {}", e);
        }
    }
}
