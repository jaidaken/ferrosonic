//! Server library changes: cache reset, full refetch, and a `getScanStatus` watcher.

use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

use tracing::{debug, info, warn};

use crate::daemon::core::DaemonCore;
use crate::ipc::protocol::DaemonEvent;
use crate::subsonic::models::ScanStatus;

/// Interval between `getScanStatus` polls.
pub const SCAN_POLL_INTERVAL: Duration = Duration::from_secs(30);

/// Watcher memory across polls: the last settled scan result and its server config.
#[derive(Debug, Default)]
pub struct ScanWatch {
    baseline: Option<(u64, ScanStatus)>,
    /// Server config generation of a scan seen running since the last settled refresh.
    scan_seen: Option<u64>,
    failing: bool,
}

/// Outcome of one `getScanStatus` poll.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScanPoll {
    /// No server configured, the request failed, or the server changed mid-request.
    Unavailable,
    /// First settled result for this server; recorded, no refresh.
    Baseline,
    /// A scan runs; the refresh waits until it ends.
    Scanning,
    /// Same result as the last settled scan.
    Unchanged,
    /// A finished scan changed the result; the library was reset and refetched.
    Refreshed,
    /// A finished scan changed the result but the refetch failed; the next poll retries.
    RefreshFailed,
}

/// Classify `status` against the settled baseline recorded for `config_gen`.
/// With no baseline yet, a scan seen running for this config means the startup
/// fetch may predate music it added, so its end refreshes.
fn classify(
    baseline: Option<&(u64, ScanStatus)>,
    scan_seen: Option<u64>,
    config_gen: u64,
    status: &ScanStatus,
) -> ScanPoll {
    if status.scanning {
        return ScanPoll::Scanning;
    }
    match baseline {
        Some((gen, last)) if *gen == config_gen => {
            if last == status {
                ScanPoll::Unchanged
            } else {
                ScanPoll::Refreshed
            }
        }
        _ if scan_seen == Some(config_gen) => ScanPoll::Refreshed,
        _ => ScanPoll::Baseline,
    }
}

impl DaemonCore {
    /// Void every album, album-track and playlist-track cache plus cover art,
    /// then tell subscribers to drop their mirrors of them.
    pub async fn invalidate_library_caches(&self) {
        {
            let mut state = self.state.write().await;
            let lib = &mut state.library;
            lib.albums_cache.clear();
            lib.albums_cache_order.clear();
            lib.album_songs_cache.clear();
            lib.album_songs_cache_order.clear();
            lib.playlist_songs_cache.clear();
            lib.playlist_songs_cache_order.clear();
            // Under the state lock: loaders compare it under the same lock before caching.
            self.library_gen.fetch_add(1, Ordering::Release);
            drop(state);
        }
        self.clear_cover_cache().await;
        self.emit(DaemonEvent::LibraryInvalidated);
    }

    /// Reset the library caches, then refetch starred songs, artists, playlists
    /// and libraries. True only when all four refetches stored their result.
    pub async fn refresh_library(self: &Arc<Self>) -> bool {
        self.invalidate_library_caches().await;
        // Run every refetch even after a failure: each one that lands is fresh data.
        let starred = self.refresh_starred().await;
        let artists = self.refresh_artists().await;
        let playlists = self.refresh_playlists().await;
        let folders = self.refresh_music_folders().await;
        starred && artists && playlists && folders
    }

    /// Poll `getScanStatus` once and refresh the library when a finished scan changed it.
    pub async fn poll_library_scan(self: &Arc<Self>, watch: &mut ScanWatch) -> ScanPoll {
        let Some((client, (config_gen, _))) = self.client_and_generation().await else {
            return ScanPoll::Unavailable;
        };
        let status = match client.get_scan_status().await {
            Ok(status) => status,
            Err(e) => {
                if watch.failing {
                    debug!("getScanStatus still failing: {e}");
                } else {
                    warn!("getScanStatus failed, server scans go unnoticed until it recovers: {e}");
                }
                watch.failing = true;
                return ScanPoll::Unavailable;
            }
        };
        if watch.failing {
            info!("getScanStatus recovered");
            watch.failing = false;
        }
        if self.config_gen_changed(config_gen) {
            return ScanPoll::Unavailable;
        }
        let outcome = classify(
            watch.baseline.as_ref(),
            watch.scan_seen,
            config_gen,
            &status,
        );
        match outcome {
            ScanPoll::Scanning => {
                watch.scan_seen = Some(config_gen);
                outcome
            }
            ScanPoll::Refreshed => {
                info!("Server scan changed the library; refreshing");
                if !self.refresh_library().await {
                    // Keep the old baseline and the seen scan so the next poll retries.
                    warn!("Library refresh after a server scan failed; retrying on the next poll");
                    return ScanPoll::RefreshFailed;
                }
                watch.baseline = Some((config_gen, status));
                watch.scan_seen = None;
                self.emit(DaemonEvent::Notification {
                    message: "Library updated from the server".to_string(),
                    is_error: false,
                });
                outcome
            }
            ScanPoll::Baseline | ScanPoll::Unchanged => {
                watch.baseline = Some((config_gen, status));
                watch.scan_seen = None;
                outcome
            }
            ScanPoll::Unavailable | ScanPoll::RefreshFailed => outcome,
        }
    }

    /// Spawn the task that polls the server scan status every `SCAN_POLL_INTERVAL`.
    pub fn spawn_library_watch(self: &Arc<Self>) -> tokio::task::JoinHandle<()> {
        let core = self.clone();
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(SCAN_POLL_INTERVAL);
            tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            let mut watch = ScanWatch::default();
            loop {
                tokio::select! {
                    biased;
                    () = core.shutdown_signal() => return,
                    _ = tick.tick() => {
                        core.poll_library_scan(&mut watch).await;
                    }
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{classify, ScanPoll, ScanStatus};

    fn settled(count: i64, last_scan: &str) -> ScanStatus {
        ScanStatus {
            scanning: false,
            count: Some(count),
            folder_count: Some(3),
            last_scan: Some(last_scan.to_string()),
        }
    }

    #[test]
    fn first_settled_result_is_a_baseline() {
        assert_eq!(
            classify(None, None, 0, &settled(10, "t1")),
            ScanPoll::Baseline
        );
    }

    #[test]
    fn same_settled_result_is_unchanged() {
        let base = (0, settled(10, "t1"));
        assert_eq!(
            classify(Some(&base), None, 0, &settled(10, "t1")),
            ScanPoll::Unchanged
        );
    }

    #[test]
    fn new_last_scan_time_triggers_a_refresh() {
        let base = (0, settled(10, "t1"));
        assert_eq!(
            classify(Some(&base), None, 0, &settled(10, "t2")),
            ScanPoll::Refreshed
        );
    }

    #[test]
    fn new_song_count_triggers_a_refresh_without_last_scan() {
        let mut before = settled(10, "t1");
        before.last_scan = None;
        let mut after = settled(12, "t1");
        after.last_scan = None;
        assert_eq!(
            classify(Some(&(0, before)), None, 0, &after),
            ScanPoll::Refreshed
        );
    }

    #[test]
    fn running_scan_defers_the_refresh_even_when_values_moved() {
        let base = (0, settled(10, "t1"));
        let mut mid = settled(99, "t2");
        mid.scanning = true;
        assert_eq!(classify(Some(&base), None, 0, &mid), ScanPoll::Scanning);
    }

    #[test]
    fn baseline_from_another_server_is_replaced_not_refreshed() {
        let base = (0, settled(10, "t1"));
        assert_eq!(
            classify(Some(&base), None, 1, &settled(50, "t9")),
            ScanPoll::Baseline
        );
    }

    #[test]
    fn scan_seen_before_any_baseline_refreshes_when_it_settles() {
        assert_eq!(
            classify(None, Some(0), 0, &settled(10, "t1")),
            ScanPoll::Refreshed
        );
    }

    #[test]
    fn scan_seen_on_another_server_is_only_a_baseline() {
        assert_eq!(
            classify(None, Some(0), 1, &settled(10, "t1")),
            ScanPoll::Baseline
        );
    }
}
