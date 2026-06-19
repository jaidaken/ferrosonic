//! Playback tick state machine + dispatch.

use std::sync::atomic::Ordering;
use std::sync::Arc;

use tracing::{debug, info, warn};

use crate::daemon::core::DaemonCore;
use crate::ipc::protocol::DaemonEvent;

/// Owned snapshot of every read the playback tick needs to decide an action.
#[derive(Debug, Clone, Copy, PartialEq)]
struct PlaybackTickInputs {
    is_active: bool,
    is_playing: bool,
    mpv_running: bool,
    time_remaining: f64,
    has_next: bool,
    position: f64,
    playlist_count: Option<usize>,
    playlist_pos: Option<i64>,
    mpv_idle: Option<bool>,
    queue_position: Option<usize>,
    prebuffer_loading: bool,
    just_loaded: bool,
}

/// Outcome of one playback tick. Branch priority: AdvanceEarly > Preload > GaplessAdvance > AdvanceOnIdle.
#[must_use]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PlaybackTickAction {
    Skip,
    AdvanceEarly,
    Preload { from_pos: usize },
    GaplessAdvance,
    AdvanceOnIdle,
    Continue,
}

/// Whether the orchestrator should fall through to the tail tick updates after the main action.
#[must_use]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TickContinuation {
    Stop,
    Continue,
}

/// Result of try_gapless_advance under the write critical section.
#[must_use]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GaplessOutcome {
    Advanced,
    QueueRanOut,
}

impl DaemonCore {
    /// Collect every read the playback tick state machine needs. Read-only by design.
    async fn gather_playback_tick_inputs(self: &Arc<Self>) -> PlaybackTickInputs {
        use crate::daemon::state::PlaybackState;

        let (is_playing, is_active) = {
            let state = self.state.read().await;
            let pl = state.now_playing.state == PlaybackState::Playing;
            let active = pl || state.now_playing.state == PlaybackState::Paused;
            (pl, active)
        };

        let mpv_running = {
            let mut mpv = self.mpv.lock().await;
            mpv.is_running()
        };

        if !is_active || !mpv_running {
            return PlaybackTickInputs {
                is_active,
                is_playing,
                mpv_running,
                time_remaining: 0.0,
                has_next: false,
                position: 0.0,
                playlist_count: None,
                playlist_pos: None,
                mpv_idle: None,
                queue_position: None,
                prebuffer_loading: false,
                just_loaded: false,
            };
        }

        let (time_remaining, has_next, position, queue_position) = {
            let state = self.state.read().await;
            let tr = state.now_playing.duration - state.now_playing.position;
            let hn = state
                .queue_position
                .map(|p| p + 1 < state.queue.len())
                .unwrap_or(false);
            (tr, hn, state.now_playing.position, state.queue_position)
        };

        let (playlist_count, playlist_pos, mpv_idle) = {
            let mut mpv = self.mpv.lock().await;
            let c = mpv.get_playlist_count().await.ok();
            let p = mpv.get_playlist_pos().await.ok().flatten();
            let i = mpv.is_idle().await.ok();
            (c, p, i)
        };

        let prebuffer_loading = self
            .prebuffer_loading
            .lock()
            .await
            .as_ref()
            .map(|a| a.load(Ordering::Acquire))
            .unwrap_or(false);

        let just_loaded = self
            .last_loadfile
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .map(|t| t.elapsed() < std::time::Duration::from_millis(1500))
            .unwrap_or(false);

        PlaybackTickInputs {
            is_active,
            is_playing,
            mpv_running,
            time_remaining,
            has_next,
            position,
            playlist_count,
            playlist_pos,
            mpv_idle,
            queue_position,
            prebuffer_loading,
            just_loaded,
        }
    }

    /// Pure state-machine. Priority: AdvanceEarly > Preload > GaplessAdvance > AdvanceOnIdle.
    const fn decide_playback_tick_action(inputs: &PlaybackTickInputs) -> PlaybackTickAction {
        if !inputs.is_active || !inputs.mpv_running {
            return PlaybackTickAction::Skip;
        }
        if !inputs.is_playing {
            return PlaybackTickAction::Continue;
        }

        if inputs.has_next
            && inputs.position > 0.5
            && inputs.time_remaining > 0.0
            && inputs.time_remaining < 2.0
        {
            if let Some(c) = inputs.playlist_count {
                if c < 2 {
                    return PlaybackTickAction::AdvanceEarly;
                }
            }
        }

        if matches!(inputs.playlist_count, Some(1)) {
            if let Some(from_pos) = inputs.queue_position {
                return PlaybackTickAction::Preload { from_pos };
            }
        }

        if matches!(inputs.playlist_pos, Some(1)) {
            return PlaybackTickAction::GaplessAdvance;
        }

        if matches!(inputs.mpv_idle, Some(true)) && !inputs.prebuffer_loading && !inputs.just_loaded
        {
            return PlaybackTickAction::AdvanceOnIdle;
        }

        PlaybackTickAction::Continue
    }

    /// Verify-then-commit a gapless advance: under one write lock, re-derive the next song and atomically swap state. Returns whether the advance happened.
    /// Test seam: run a gapless advance and discard the internal outcome.
    #[doc(hidden)]
    pub async fn try_gapless_advance_for_test(self: &Arc<Self>) {
        let _ = self.try_gapless_advance().await;
    }

    async fn try_gapless_advance(self: &Arc<Self>) -> GaplessOutcome {
        // DIAG (temporary): mpv state at the gapless trigger; a wrong-track
        // advance shows as mpv dur not matching the expected next-song dur.
        let (mpos, mcount, mdur, mtime) = {
            let mut mpv = self.mpv.lock().await;
            (
                mpv.get_playlist_pos().await.ok().flatten(),
                mpv.get_playlist_count().await.ok(),
                mpv.get_duration().await.unwrap_or(0.0),
                mpv.get_time_pos().await.unwrap_or(0.0),
            )
        };
        let (next_pos, diag) = {
            let mut state = self.state.write().await;
            let queue_len = state.queue.len();
            let repeat = state.config.repeat_mode;
            let cur = state.queue_position;
            let now_title = state
                .now_playing
                .song
                .as_ref()
                .map(|x| x.title.clone())
                .unwrap_or_default();
            let resolved = cur.and_then(|c| {
                repeat
                    .next_auto(c, queue_len)
                    .and_then(|n| state.queue.get(n).map(|s| (n, s.clone())))
            });
            if let Some((next_pos, song)) = resolved {
                let diag = (
                    cur,
                    now_title,
                    song.title.clone(),
                    song.duration.unwrap_or(0),
                );
                state.queue_position = Some(next_pos);
                state.now_playing.song = Some(song.clone());
                state.now_playing.position = 0.0;
                state.now_playing.duration = song.duration.unwrap_or(0) as f64;
                // Clear so the tick re-probes + re-pins; a gapless jump across
                // sample rates must not stay pinned to the previous track's rate.
                state.now_playing.sample_rate = None;
                state.now_playing.bit_depth = None;
                (Some(next_pos), Some(diag))
            } else {
                (None, None)
            }
        };
        if let Some((cur, now, next_title, next_dur)) = &diag {
            info!(
                "GAPLESS-DIAG mpv[pos={:?} count={:?} dur={:.0}s time={:.0}s] queue[pos={:?} now='{}' next='{}' next_dur={}s]",
                mpos, mcount, mdur, mtime, cur, now, next_title, next_dur
            );
        }
        let Some(next_pos) = next_pos else {
            return GaplessOutcome::QueueRanOut;
        };
        info!("Gapless advancement to track {}", next_pos);
        {
            let mut mpv = self.mpv.lock().await;
            let pos_now = mpv.get_playlist_pos().await.ok().flatten();
            if matches!(pos_now, Some(1)) {
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
        self.emit_queue().await;
        // Re-clock near the boundary: a gapless jump across sample rates
        // must re-pin instead of riding the 500ms backstop tick.
        self.spawn_fast_probe();
        GaplessOutcome::Advanced
    }

    /// Rate-limit on `last_preload_attempt`; returns true when due (and bumps the timer).
    fn bump_preload_due(&self) -> bool {
        let mut last = self
            .last_preload_attempt
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let due = last
            .map(|t| t.elapsed() >= std::time::Duration::from_secs(5))
            .unwrap_or(true);
        if due {
            *last = Some(std::time::Instant::now());
        }
        due
    }

    /// Dispatch the decided action. Returns Stop when the orchestrator should NOT run tail tick updates.
    async fn apply_playback_tick_action(
        self: &Arc<Self>,
        action: PlaybackTickAction,
    ) -> TickContinuation {
        match action {
            PlaybackTickAction::Skip => TickContinuation::Stop,
            PlaybackTickAction::AdvanceEarly => {
                info!("Near end of track with no preloaded next, advancing early");
                let _ = self.advance_auto().await;
                TickContinuation::Stop
            }
            PlaybackTickAction::Preload { from_pos } => {
                if self.bump_preload_due() {
                    debug!("Playlist count is 1, re-preloading next track");
                    self.preload_next_track(from_pos).await;
                }
                TickContinuation::Continue
            }
            PlaybackTickAction::GaplessAdvance => match self.try_gapless_advance().await {
                GaplessOutcome::Advanced => TickContinuation::Stop,
                GaplessOutcome::QueueRanOut => TickContinuation::Continue,
            },
            PlaybackTickAction::AdvanceOnIdle => {
                info!("Track ended, advancing to next");
                let _ = self.advance_auto().await;
                TickContinuation::Stop
            }
            PlaybackTickAction::Continue => TickContinuation::Continue,
        }
    }

    /// Emit a PositionTick event with mpv's current playhead.
    async fn tick_emit_position(self: &Arc<Self>) {
        use crate::daemon::state::PlaybackState;
        let pos_opt = {
            let mut mpv = self.mpv.lock().await;
            mpv.get_time_pos().await.ok()
        };
        if let Some(position) = pos_opt {
            // Only track position while Playing; a stopped-for-pause mpv reports 0 and would clobber the saved resume point.
            let emit = {
                let mut state = self.state.write().await;
                if state.now_playing.state == PlaybackState::Playing {
                    state.now_playing.position = position;
                    true
                } else {
                    false
                }
            };
            if emit {
                self.emit(DaemonEvent::PositionTick(position));
            }
        }
    }

    /// Backfill duration if missing; re-check is INSIDE the write critical section to close a TOCTOU.
    async fn tick_backfill_duration(self: &Arc<Self>) {
        let dur_opt = {
            let mut mpv = self.mpv.lock().await;
            mpv.get_duration().await.ok()
        };
        let Some(dur) = dur_opt.filter(|&d| d > 0.0) else {
            return;
        };
        let mut state = self.state.write().await;
        if state.now_playing.duration <= 0.0 {
            state.now_playing.duration = dur;
        }
    }

    /// Fire a desktop notification once per track change while Playing, off the
    /// tick so the cover-art fetch never blocks playback. No-op when the
    /// notifications config is off or no session bus is reachable.
    async fn tick_desktop_notification(self: &Arc<Self>) {
        use crate::daemon::state::PlaybackState;
        let (enabled, song) = {
            let s = self.state.read().await;
            if s.now_playing.state != PlaybackState::Playing {
                return;
            }
            (s.config.notifications, s.now_playing.song.clone())
        };
        let Some(song) = song.filter(|_| enabled) else {
            return;
        };
        if !self.notifier.mark_if_changed(&song.id) {
            return;
        }
        let core = self.clone();
        tokio::spawn(async move {
            if core.shutdown.load(std::sync::atomic::Ordering::Acquire) {
                return;
            }
            let cover = match song.cover_art.as_deref() {
                Some(cid) => {
                    let bytes = core.get_cover_art(cid, 512).await;
                    (!bytes.is_empty()).then_some(bytes)
                }
                None => None,
            };
            let body = crate::daemon::notify::track_body(&song);
            core.notifier
                .show(&song.title, &body, cover.as_deref())
                .await;
        });
    }

    /// Fetch sample-rate + bit-depth + format + channels if not yet known. Backstop poll.
    async fn tick_fetch_audio_properties_if_needed(self: &Arc<Self>) {
        let need_sr = self.state.read().await.now_playing.sample_rate.is_none();
        if need_sr {
            let _ = self.fetch_audio_properties().await;
        }
    }

    /// 500ms tick: gather inputs, decide an action, apply it, then run tail updates unless told to stop.
    pub async fn update_playback_info(self: &Arc<Self>) {
        let inputs = self.gather_playback_tick_inputs().await;
        let action = Self::decide_playback_tick_action(&inputs);
        let cont = self.apply_playback_tick_action(action).await;
        // Runs every tick, including Skip/Stop, so a stop or end-of-queue still
        // finalizes the played track (the modern path reports "stopped" there).
        self.scrobble_tick().await;
        self.tick_desktop_notification().await;
        if matches!(cont, TickContinuation::Stop) {
            return;
        }
        self.tick_emit_position().await;
        self.tick_backfill_duration().await;
        self.tick_fetch_audio_properties_if_needed().await;
    }
}

#[cfg(test)]
mod playback_tick_tests {
    use super::{GaplessOutcome, PlaybackTickAction, PlaybackTickInputs, TickContinuation};
    use crate::daemon::core::DaemonCore;

    fn baseline() -> PlaybackTickInputs {
        PlaybackTickInputs {
            is_active: true,
            is_playing: true,
            mpv_running: true,
            time_remaining: 100.0,
            has_next: false,
            position: 50.0,
            playlist_count: Some(2),
            playlist_pos: Some(0),
            mpv_idle: Some(false),
            queue_position: Some(0),
            prebuffer_loading: false,
            just_loaded: false,
        }
    }

    fn decide(i: &PlaybackTickInputs) -> PlaybackTickAction {
        DaemonCore::decide_playback_tick_action(i)
    }

    #[test]
    fn skip_when_inactive() {
        let i = PlaybackTickInputs {
            is_active: false,
            ..baseline()
        };
        assert_eq!(decide(&i), PlaybackTickAction::Skip);
    }

    #[test]
    fn skip_when_mpv_not_running() {
        let i = PlaybackTickInputs {
            mpv_running: false,
            ..baseline()
        };
        assert_eq!(decide(&i), PlaybackTickAction::Skip);
    }

    #[test]
    fn continue_when_paused() {
        let i = PlaybackTickInputs {
            is_playing: false,
            ..baseline()
        };
        assert_eq!(decide(&i), PlaybackTickAction::Continue);
    }

    #[test]
    fn advance_early_at_track_end_with_empty_playlist() {
        let i = PlaybackTickInputs {
            has_next: true,
            position: 1.0,
            time_remaining: 1.0,
            playlist_count: Some(1),
            ..baseline()
        };
        assert_eq!(decide(&i), PlaybackTickAction::AdvanceEarly);
    }

    #[test]
    fn preload_when_playlist_one_and_position_present() {
        let i = PlaybackTickInputs {
            has_next: false,
            playlist_count: Some(1),
            queue_position: Some(3),
            ..baseline()
        };
        assert_eq!(decide(&i), PlaybackTickAction::Preload { from_pos: 3 });
    }

    #[test]
    fn gapless_advance_when_mpv_playlist_pos_one() {
        let i = PlaybackTickInputs {
            playlist_count: Some(2),
            playlist_pos: Some(1),
            ..baseline()
        };
        assert_eq!(decide(&i), PlaybackTickAction::GaplessAdvance);
    }

    #[test]
    fn advance_on_idle_when_idle_and_clean() {
        let i = PlaybackTickInputs {
            playlist_pos: Some(0),
            mpv_idle: Some(true),
            prebuffer_loading: false,
            just_loaded: false,
            ..baseline()
        };
        assert_eq!(decide(&i), PlaybackTickAction::AdvanceOnIdle);
    }

    #[test]
    fn continue_when_playing_and_nothing_matches() {
        let i = baseline();
        assert_eq!(decide(&i), PlaybackTickAction::Continue);
    }

    #[test]
    fn priority_advance_early_beats_preload() {
        let i = PlaybackTickInputs {
            has_next: true,
            position: 1.0,
            time_remaining: 1.0,
            playlist_count: Some(1),
            queue_position: Some(0),
            ..baseline()
        };
        assert_eq!(decide(&i), PlaybackTickAction::AdvanceEarly);
    }

    #[test]
    fn priority_preload_beats_gapless_advance() {
        let i = PlaybackTickInputs {
            playlist_count: Some(1),
            playlist_pos: Some(1),
            queue_position: Some(7),
            ..baseline()
        };
        assert_eq!(decide(&i), PlaybackTickAction::Preload { from_pos: 7 });
    }

    #[test]
    fn priority_gapless_advance_beats_idle() {
        let i = PlaybackTickInputs {
            playlist_pos: Some(1),
            mpv_idle: Some(true),
            ..baseline()
        };
        assert_eq!(decide(&i), PlaybackTickAction::GaplessAdvance);
    }

    #[test]
    fn idle_suppressed_by_prebuffer_loading() {
        let i = PlaybackTickInputs {
            playlist_pos: Some(0),
            mpv_idle: Some(true),
            prebuffer_loading: true,
            ..baseline()
        };
        assert_eq!(decide(&i), PlaybackTickAction::Continue);
    }

    #[test]
    fn idle_suppressed_by_just_loaded() {
        let i = PlaybackTickInputs {
            playlist_pos: Some(0),
            mpv_idle: Some(true),
            just_loaded: true,
            ..baseline()
        };
        assert_eq!(decide(&i), PlaybackTickAction::Continue);
    }

    #[test]
    fn advance_early_requires_position_above_half() {
        let i = PlaybackTickInputs {
            has_next: true,
            position: 0.3,
            time_remaining: 1.0,
            playlist_count: Some(1),
            ..baseline()
        };
        assert_ne!(decide(&i), PlaybackTickAction::AdvanceEarly);
    }

    #[test]
    fn advance_early_requires_time_remaining_below_two() {
        let i = PlaybackTickInputs {
            has_next: true,
            position: 1.0,
            time_remaining: 2.5,
            playlist_count: Some(1),
            ..baseline()
        };
        assert_ne!(decide(&i), PlaybackTickAction::AdvanceEarly);
    }

    #[test]
    fn advance_early_excludes_position_exactly_half() {
        let i = PlaybackTickInputs {
            has_next: true,
            position: 0.5,
            time_remaining: 1.0,
            playlist_count: Some(1),
            queue_position: Some(0),
            ..baseline()
        };
        assert_ne!(decide(&i), PlaybackTickAction::AdvanceEarly);
    }

    #[test]
    fn advance_early_excludes_zero_time_remaining() {
        let i = PlaybackTickInputs {
            has_next: true,
            position: 1.0,
            time_remaining: 0.0,
            playlist_count: Some(1),
            queue_position: Some(0),
            ..baseline()
        };
        assert_ne!(decide(&i), PlaybackTickAction::AdvanceEarly);
    }

    #[test]
    fn advance_early_excludes_time_remaining_exactly_two() {
        let i = PlaybackTickInputs {
            has_next: true,
            position: 1.0,
            time_remaining: 2.0,
            playlist_count: Some(1),
            queue_position: Some(0),
            ..baseline()
        };
        assert_ne!(decide(&i), PlaybackTickAction::AdvanceEarly);
    }

    #[test]
    fn advance_early_excludes_playlist_count_exactly_two() {
        let i = PlaybackTickInputs {
            has_next: true,
            position: 1.0,
            time_remaining: 1.0,
            playlist_count: Some(2),
            ..baseline()
        };
        assert_ne!(decide(&i), PlaybackTickAction::AdvanceEarly);
    }

    #[test]
    fn enum_must_use_attrs_present() {
        assert_eq!(TickContinuation::Stop, TickContinuation::Stop);
        assert_ne!(TickContinuation::Stop, TickContinuation::Continue);
        assert_eq!(GaplessOutcome::Advanced, GaplessOutcome::Advanced);
        assert_ne!(GaplessOutcome::Advanced, GaplessOutcome::QueueRanOut);
    }
}

#[cfg(test)]
mod prop {
    use super::*;
    use proptest::prelude::*;

    prop_compose! {
        fn arb_inputs()(
            is_active in any::<bool>(),
            is_playing in any::<bool>(),
            mpv_running in any::<bool>(),
            time_remaining in -10.0f64..120.0,
            has_next in any::<bool>(),
            position in -1.0f64..600.0,
            playlist_count in prop_oneof![Just(None), (0usize..8).prop_map(Some)],
            playlist_pos in prop_oneof![Just(None), (-1i64..8).prop_map(Some)],
            mpv_idle in prop_oneof![Just(None), any::<bool>().prop_map(Some)],
            queue_position in prop_oneof![Just(None), (0usize..32).prop_map(Some)],
            prebuffer_loading in any::<bool>(),
            just_loaded in any::<bool>(),
        ) -> PlaybackTickInputs {
            PlaybackTickInputs {
                is_active,
                is_playing,
                mpv_running,
                time_remaining,
                has_next,
                position,
                playlist_count,
                playlist_pos,
                mpv_idle,
                queue_position,
                prebuffer_loading,
                just_loaded,
            }
        }
    }

    fn is_known_variant(action: PlaybackTickAction) -> bool {
        matches!(
            action,
            PlaybackTickAction::Skip
                | PlaybackTickAction::AdvanceEarly
                | PlaybackTickAction::Preload { .. }
                | PlaybackTickAction::GaplessAdvance
                | PlaybackTickAction::AdvanceOnIdle
                | PlaybackTickAction::Continue
        )
    }

    proptest! {
        #[test]
        fn never_panics_on_arbitrary_inputs(inputs in arb_inputs()) {
            let action = DaemonCore::decide_playback_tick_action(&inputs);
            prop_assert!(is_known_variant(action));
        }

        #[test]
        fn preload_only_when_playlist_count_one(inputs in arb_inputs()) {
            let action = DaemonCore::decide_playback_tick_action(&inputs);
            if let PlaybackTickAction::Preload { from_pos } = action {
                prop_assert_eq!(inputs.playlist_count, Some(1));
                prop_assert_eq!(Some(from_pos), inputs.queue_position);
                prop_assert!(inputs.is_active);
                prop_assert!(inputs.is_playing);
                prop_assert!(inputs.mpv_running);
            }
        }

        #[test]
        fn action_preconditions_hold(inputs in arb_inputs()) {
            let action = DaemonCore::decide_playback_tick_action(&inputs);
            match action {
                PlaybackTickAction::Skip => {
                    prop_assert!(!inputs.is_active || !inputs.mpv_running);
                }
                PlaybackTickAction::AdvanceEarly => {
                    prop_assert!(inputs.is_active && inputs.mpv_running && inputs.is_playing);
                    prop_assert!(inputs.has_next);
                    prop_assert!(inputs.position > 0.5);
                    prop_assert!(inputs.time_remaining > 0.0 && inputs.time_remaining < 2.0);
                    prop_assert!(matches!(inputs.playlist_count, Some(c) if c < 2));
                }
                PlaybackTickAction::Preload { .. } => {
                    prop_assert!(inputs.is_active && inputs.mpv_running && inputs.is_playing);
                    prop_assert_eq!(inputs.playlist_count, Some(1));
                    prop_assert!(inputs.queue_position.is_some());
                }
                PlaybackTickAction::GaplessAdvance => {
                    prop_assert!(inputs.is_active && inputs.mpv_running && inputs.is_playing);
                    prop_assert_eq!(inputs.playlist_pos, Some(1));
                }
                PlaybackTickAction::AdvanceOnIdle => {
                    prop_assert!(inputs.is_active && inputs.mpv_running && inputs.is_playing);
                    prop_assert_eq!(inputs.mpv_idle, Some(true));
                    prop_assert!(!inputs.prebuffer_loading);
                    prop_assert!(!inputs.just_loaded);
                }
                PlaybackTickAction::Continue => {}
            }
        }

        #[test]
        fn priority_preserved_when_advance_early_eligible(
            from_pos in 0usize..16,
            playlist_pos_val in 0i64..4,
        ) {
            let inputs = PlaybackTickInputs {
                is_active: true,
                is_playing: true,
                mpv_running: true,
                has_next: true,
                position: 1.0,
                time_remaining: 1.0,
                playlist_count: Some(1),
                queue_position: Some(from_pos),
                playlist_pos: Some(playlist_pos_val),
                mpv_idle: Some(true),
                prebuffer_loading: false,
                just_loaded: false,
            };
            let action = DaemonCore::decide_playback_tick_action(&inputs);
            prop_assert_eq!(action, PlaybackTickAction::AdvanceEarly);
        }

        #[test]
        fn priority_preload_beats_lower_when_eligible(
            from_pos in 0usize..16,
            time_remaining in 5.0f64..60.0,
        ) {
            let inputs = PlaybackTickInputs {
                is_active: true,
                is_playing: true,
                mpv_running: true,
                has_next: false,
                time_remaining,
                position: 10.0,
                playlist_count: Some(1),
                queue_position: Some(from_pos),
                playlist_pos: Some(1),
                mpv_idle: Some(true),
                prebuffer_loading: false,
                just_loaded: false,
            };
            let action = DaemonCore::decide_playback_tick_action(&inputs);
            prop_assert_eq!(action, PlaybackTickAction::Preload { from_pos });
        }
    }
}
