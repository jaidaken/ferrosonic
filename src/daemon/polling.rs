//! Background tokio task spawns: 500ms playback tick + debounced queue persistence.

use std::sync::Arc;

use tracing::{error, warn};

use crate::daemon::core::DaemonCore;
use crate::daemon::persistence::QueueSnapshot;

/// Persist the queue snapshot on the blocking pool: `save` does
/// `create_dir_all` plus an fsync-backed atomic write, which must not run
/// directly on an async worker.
async fn save_snapshot_off_thread(
    snap: QueueSnapshot,
) -> Result<std::path::PathBuf, std::io::Error> {
    tokio::task::spawn_blocking(move || snap.save())
        .await
        .map_err(|e| std::io::Error::other(format!("queue save task failed: {e}")))?
}

impl DaemonCore {
    pub(super) fn spawn_queue_persistence(
        self: Arc<Self>,
        mut rx: tokio::sync::mpsc::Receiver<()>,
    ) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    () = self.shutdown_signal() => return,
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
                if let Err(e) = save_snapshot_off_thread(snap).await {
                    warn!("Queue persistence write failed: {}", e);
                }
            }
        })
    }

    /// Spawn the 500ms tick task: position ticks, idle-advance, watchdog.
    pub fn spawn_polling_task(self: &Arc<Self>) -> tokio::task::JoinHandle<()> {
        let core = self.clone();
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(std::time::Duration::from_millis(500));
            let mut watchdog = tokio::time::interval(std::time::Duration::from_secs(2));
            tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            watchdog.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                tokio::select! {
                    () = core.shutdown_signal() => return,
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
}
