//! Dual-path play reporting driven from the playback tick: classic `scrobble` or OpenSubsonic `reportPlayback`.

use std::sync::atomic::Ordering;
use std::sync::Arc;

use tracing::debug;

use crate::daemon::core::DaemonCore;
use crate::daemon::state::PlaybackState;

/// Per-play scrobble tracking, compared against the latest tick to detect
/// transitions. Reset on every track change.
#[derive(Default)]
pub struct ScrobbleState {
    song_id: Option<String>,
    duration: f64,
    last_position: f64,
    last_state: PlaybackState,
    submitted: bool,
    now_playing_sent: bool,
}

/// One outbound report the tick decided to send.
enum ScrobbleHttp {
    NowPlaying(String),
    Submission(String),
    Report {
        id: String,
        position_ms: u64,
        state: &'static str,
    },
}

/// A play counts once the playhead passes half its length or four minutes,
/// whichever is first; tracks shorter than 31s never count (Last.fm rule).
fn classic_reached(position: f64, duration: f64) -> bool {
    if duration > 0.0 && duration <= 30.0 {
        return false;
    }
    let threshold = if duration > 0.0 {
        (duration * 0.5).min(240.0)
    } else {
        240.0
    };
    position >= threshold
}

fn position_ms(position: f64) -> u64 {
    (position.max(0.0) * 1000.0) as u64
}

impl DaemonCore {
    /// Query the server's OpenSubsonic extensions and record whether
    /// `playbackReport` is available; selects the modern vs classic path.
    pub(super) fn spawn_refresh_scrobble_capability(self: &Arc<Self>) {
        if self.shutdown.load(Ordering::Acquire) {
            return;
        }
        let core = self.clone();
        tokio::spawn(async move {
            if core.shutdown.load(Ordering::Acquire) {
                return;
            }
            let Some(client) = core.subsonic.read().await.clone() else {
                return;
            };
            let supported = client
                .get_open_subsonic_extensions()
                .await
                .map(|exts| exts.iter().any(|e| e == "playbackReport"))
                .unwrap_or(false);
            core.playback_report_supported
                .store(supported, Ordering::Release);
            debug!("playbackReport extension supported: {supported}");
        });
    }

    /// Test seam: force the detected `playbackReport` capability.
    #[doc(hidden)]
    pub fn set_playback_report_for_test(&self, supported: bool) {
        self.playback_report_supported
            .store(supported, Ordering::Release);
    }

    /// Drive the scrobble state machine from the current now-playing snapshot.
    /// Reads state briefly, decides under the scrobble lock, then fires the
    /// resulting reports off the tick path. No HTTP is awaited while a lock is
    /// held. Gated by `config.scrobble`.
    pub async fn scrobble_tick(self: &Arc<Self>) {
        let (enabled, id, state, position, duration) = {
            let s = self.state.read().await;
            (
                s.config.scrobble,
                s.now_playing.song.as_ref().map(|c| c.id.clone()),
                s.now_playing.state,
                s.now_playing.position,
                s.now_playing.duration,
            )
        };
        let modern = self.playback_report_supported.load(Ordering::Acquire);

        let mut actions: Vec<ScrobbleHttp> = Vec::new();
        {
            let mut t = self.scrobble_state.lock().await;

            if !enabled {
                *t = ScrobbleState::default();
            } else if t.song_id != id {
                // Finalize the track we were on before switching to the new one.
                if let Some(prev) = t.song_id.clone() {
                    if modern {
                        actions.push(ScrobbleHttp::Report {
                            id: prev,
                            position_ms: position_ms(t.last_position),
                            state: "stopped",
                        });
                    } else if !t.submitted && classic_reached(t.last_position, t.duration) {
                        actions.push(ScrobbleHttp::Submission(prev));
                    }
                }
                *t = ScrobbleState {
                    song_id: id.clone(),
                    duration,
                    last_position: position,
                    last_state: state,
                    ..Default::default()
                };
                if let Some(nid) = id.clone() {
                    if state == PlaybackState::Playing {
                        t.now_playing_sent = true;
                        actions.push(start_action(nid, position, modern));
                    }
                }
            } else if let Some(nid) = id.clone() {
                if state != t.last_state {
                    if modern {
                        let label = match state {
                            PlaybackState::Playing => "playing",
                            PlaybackState::Paused => "paused",
                            PlaybackState::Stopped => "stopped",
                        };
                        actions.push(ScrobbleHttp::Report {
                            id: nid.clone(),
                            position_ms: position_ms(position),
                            state: label,
                        });
                    } else if state == PlaybackState::Playing && !t.now_playing_sent {
                        t.now_playing_sent = true;
                        actions.push(ScrobbleHttp::NowPlaying(nid.clone()));
                    }
                    t.last_state = state;
                }
                if !modern
                    && state == PlaybackState::Playing
                    && !t.submitted
                    && classic_reached(position, duration)
                {
                    t.submitted = true;
                    actions.push(ScrobbleHttp::Submission(nid));
                }
                t.last_position = position;
                if duration > 0.0 {
                    t.duration = duration;
                }
            }
        }

        for action in actions {
            self.spawn_scrobble(action);
        }
    }

    fn spawn_scrobble(self: &Arc<Self>, action: ScrobbleHttp) {
        if self.shutdown.load(Ordering::Acquire) {
            return;
        }
        let core = self.clone();
        tokio::spawn(async move {
            if core.shutdown.load(Ordering::Acquire) {
                return;
            }
            let Some(client) = core.subsonic.read().await.clone() else {
                return;
            };
            let res = match action {
                ScrobbleHttp::NowPlaying(id) => client.scrobble(&id, false, None).await,
                ScrobbleHttp::Submission(id) => client.scrobble(&id, true, None).await,
                ScrobbleHttp::Report {
                    id,
                    position_ms,
                    state,
                } => client.report_playback(&id, position_ms, state, false).await,
            };
            if let Err(e) = res {
                debug!("scrobble report failed: {e}");
            }
        });
    }
}

fn start_action(id: String, position: f64, modern: bool) -> ScrobbleHttp {
    if modern {
        ScrobbleHttp::Report {
            id,
            position_ms: position_ms(position),
            state: "playing",
        }
    } else {
        ScrobbleHttp::NowPlaying(id)
    }
}

#[cfg(test)]
mod tests {
    use super::classic_reached;

    #[test]
    fn counts_a_play_at_half_a_short_track() {
        assert!(!classic_reached(149.0, 300.0));
        assert!(classic_reached(150.0, 300.0));
    }

    #[test]
    fn caps_the_threshold_at_four_minutes_for_long_tracks() {
        assert!(!classic_reached(239.0, 600.0));
        assert!(classic_reached(240.0, 600.0));
    }

    #[test]
    fn never_counts_a_track_under_31s() {
        assert!(!classic_reached(24.0, 25.0));
    }

    #[test]
    fn unknown_duration_falls_back_to_four_minutes() {
        assert!(!classic_reached(239.0, 0.0));
        assert!(classic_reached(240.0, 0.0));
    }
}
