//! Playback control: pause/resume, seek, skip, advance, preload, stop, volume.

use std::sync::Arc;

use tracing::{debug, error, info, warn};

use crate::daemon::core::{DaemonCore, PlayMode};
use crate::error::Error;

/// Clears `DaemonCore::advance_in_flight` on drop, so every exit path (early
/// return, `?`, or panic) releases the single-flight guard.
struct AdvanceGuard<'a>(&'a std::sync::atomic::AtomicBool);

#[derive(Clone, Copy)]
enum PlayIntent {
    Start,
    Resume,
}

impl Drop for AdvanceGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, std::sync::atomic::Ordering::Release);
    }
}

impl DaemonCore {
    /// Toggle pause by current state: `Playing` pauses, `Paused` resumes, `Stopped` with a queued position starts playback. Delegates so the `PipeWire` pin release/re-apply lives in one place per direction.
    ///
    /// # Errors
    /// Returns an `Error` if mpv control or a server request fails.
    pub async fn toggle_pause(self: &Arc<Self>) -> Result<(), Error> {
        use crate::daemon::state::PlaybackState;
        let (playback_state, queue_pos) = {
            let state = self.state.read().await;
            (state.now_playing.state, state.queue_position)
        };
        match playback_state {
            PlaybackState::Playing => self.pause_playback().await,
            PlaybackState::Paused => self.resume_playback().await,
            PlaybackState::Stopped => match queue_pos {
                Some(pos) => self.play_queue_position(pos, PlayMode::Direct).await,
                None => Ok(()),
            },
        }
    }

    /// Pause playback. Stops mpv so it disconnects its `PipeWire` stream and
    /// releases the force-rate pin, so the audio device follows other apps
    /// (e.g. a browser) while paused. The playhead is kept in
    /// `now_playing.position`; resume re-pins the known rate then reloads and
    /// seeks back. Commits `Paused` before the stop so the idle tick (gated on
    /// `is_playing`) cannot read the stop as a track-end and auto-advance.
    ///
    /// # Errors
    /// Returns an `Error` if mpv control or a server request fails.
    pub async fn pause_playback(self: &Arc<Self>) -> Result<(), Error> {
        use crate::daemon::state::PlaybackState;
        let was_playing = {
            let mut state = self.state.write().await;
            if state.now_playing.state == PlaybackState::Playing {
                state.now_playing.state = PlaybackState::Paused;
                true
            } else {
                false
            }
        };
        if !was_playing {
            return Ok(());
        }
        // Pausing must also stop an in-flight Buffered download: without this
        // the abandoned task would later loadfile and resume, playing audio
        // while the UI/MPRIS say Paused.
        self.cancel_prebuffer().await;
        {
            let mut mpv = self.mpv.lock().await;
            if let Err(e) = mpv.stop().await {
                error!("Failed to stop mpv on pause: {}", e);
            }
        }
        self.emit_now_playing().await;
        self.release_pipewire_rate().await;
        Ok(())
    }

    /// Resume from pause by reloading the current track and seeking back to the saved position (mpv was stopped on pause to free the audio device). Before audio, compares the device's current rate to the track's known rate; if they differ it switches and waits the settle delay so the re-clock finishes in silence, never in the music. From `Stopped` with a queued position, starts that track from the top.
    ///
    /// # Errors
    /// Returns an `Error` if mpv control or a server request fails.
    // significant_drop_tightening: tokio guard held to scope; not tightened (early-drop is borrow-blocked, spans a trailing await, or saves nothing before return).
    #[allow(clippy::significant_drop_tightening)]
    pub async fn resume_playback(self: &Arc<Self>) -> Result<(), Error> {
        use crate::daemon::state::PlaybackState;
        let (playback_state, queue_pos, resume_at, known_rate, settle_ms) = {
            let state = self.state.read().await;
            (
                state.now_playing.state,
                state.queue_position,
                state.now_playing.position,
                state.now_playing.sample_rate,
                state.config.rate_switch_delay_ms,
            )
        };
        if playback_state != PlaybackState::Paused && playback_state != PlaybackState::Stopped {
            return Ok(());
        }
        let Some(pos) = queue_pos else {
            return Ok(());
        };
        let start_at = if playback_state == PlaybackState::Paused {
            resume_at
        } else {
            0.0
        };
        // Pause released the pin: if the device no longer matches the track's
        // rate, switch and wait the settle so the re-clock lands in silence.
        if playback_state == PlaybackState::Paused {
            if let Some(rate) = known_rate {
                let switched = {
                    let mut pw = self.pipewire.lock().await;
                    if pw.get_current_rate() == Some(rate) {
                        false
                    } else {
                        if let Err(e) = pw.set_rate(rate).await {
                            warn!("Failed to re-pin rate on resume: {}", e);
                        }
                        true
                    }
                };
                if switched {
                    tokio::time::sleep(std::time::Duration::from_millis(u64::from(settle_ms)))
                        .await;
                }
            }
        }
        let _transition = self.playback_transitions.lock().await;
        self.play_queue_position_at_locked(pos, PlayMode::Direct, start_at, PlayIntent::Resume)
            .await?;
        Ok(())
    }

    /// Start restored playback when `AutoplayOnStart` is set and a track was
    /// restored paused. No-op otherwise. Called once at startup after mpv is
    /// running, in both daemon and standalone modes.
    pub async fn autoplay_restored_if_configured(self: &Arc<Self>) {
        use crate::daemon::state::PlaybackState;
        let (autoplay, state, pos) = {
            let s = self.state.read().await;
            (
                s.config.autoplay_on_start,
                s.now_playing.state,
                s.queue_position,
            )
        };
        if !autoplay || state != PlaybackState::Paused || pos.is_none() {
            return;
        }
        info!("Autoplay on start: resuming restored track");
        if let Err(e) = self.resume_playback().await {
            error!("Autoplay on start failed: {}", e);
        }
    }

    /// Manual skip. Ignores `repeat=One` (user wants to move).
    ///
    /// # Errors
    /// Returns an `Error` if mpv control or a server request fails.
    pub async fn next_track(self: &Arc<Self>) -> Result<(), Error> {
        let auto_was_in_flight = self
            .advance_in_flight
            .load(std::sync::atomic::Ordering::Acquire);
        let _transition = self.playback_transitions.lock().await;
        if auto_was_in_flight {
            debug!("manual Next coalesced with an in-flight automatic advance");
            return Ok(());
        }
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
        let next_pos: Option<usize> =
            current_pos.map_or(Some(0), |p| repeat.next_manual(p, queue_len));
        if let Some(p) = next_pos {
            return self
                .play_queue_position_at_locked(p, PlayMode::Direct, 0.0, PlayIntent::Start)
                .await;
        }
        if auto_continue && self.extend_with_random_and_play().await? {
            return Ok(());
        }
        self.finish_at_queue_end().await;
        Ok(())
    }

    /// Auto-end advance. Honours `repeat=One` and `repeat=All`.
    ///
    /// # Errors
    /// Returns an `Error` if mpv control or a server request fails.
    pub async fn advance_auto(self: &Arc<Self>) -> Result<(), Error> {
        // The mpv EOF listener and the 500ms idle tick can both fire for the
        // same track end; letting both resolve-and-play skips a track. Only the
        // first caller advances; the other sees the guard and returns.
        if self
            .advance_in_flight
            .compare_exchange(
                false,
                true,
                std::sync::atomic::Ordering::AcqRel,
                std::sync::atomic::Ordering::Acquire,
            )
            .is_err()
        {
            debug!("advance_auto already in flight; skipping duplicate advance");
            return Ok(());
        }
        let _advance_guard = AdvanceGuard(&self.advance_in_flight);
        let instance_at_request = self
            .play_instance
            .load(std::sync::atomic::Ordering::Acquire);
        let _transition = self.playback_transitions.lock().await;
        if self
            .play_instance
            .load(std::sync::atomic::Ordering::Acquire)
            != instance_at_request
        {
            debug!("automatic advance superseded by a manual playback transition");
            return Ok(());
        }
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
        let next_pos: Option<usize> =
            current_pos.map_or(Some(0), |p| repeat.next_auto(p, queue_len));
        if let Some(p) = next_pos {
            return self
                .play_queue_position_at_locked(p, PlayMode::Direct, 0.0, PlayIntent::Start)
                .await;
        }
        if auto_continue && self.extend_with_random_and_play().await? {
            return Ok(());
        }
        self.finish_at_queue_end().await;
        Ok(())
    }

    /// Restarts current track if more than 3s in, else goes back one.
    ///
    /// # Errors
    /// Returns an `Error` if mpv control or a server request fails.
    pub async fn prev_track(self: &Arc<Self>) -> Result<(), Error> {
        let _transition = self.playback_transitions.lock().await;
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
                    return self
                        .play_queue_position_at_locked(
                            pos - 1,
                            PlayMode::Direct,
                            0.0,
                            PlayIntent::Start,
                        )
                        .await;
                }
                if let Some(wrap_to) = repeat.prev_wrap(queue_len) {
                    return self
                        .play_queue_position_at_locked(
                            wrap_to,
                            PlayMode::Direct,
                            0.0,
                            PlayIntent::Start,
                        )
                        .await;
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

    /// Load and play the queue entry at `pos` from the start; drives the `PipeWire` rate switch.
    ///
    /// # Errors
    /// Returns an `Error` if mpv control or a server request fails.
    pub async fn play_queue_position(
        self: &Arc<Self>,
        pos: usize,
        mode: PlayMode,
    ) -> Result<(), Error> {
        self.play_queue_position_at(pos, mode, 0.0).await
    }

    /// Load and play the queue entry at `pos`, beginning at `start_at` seconds; commits `now_playing.position` to `start_at` so resume reflects the playhead before the first tick. mpv decodes from the offset (no post-load seek to race).
    ///
    /// # Errors
    ///
    /// Returns an error if the play dispatch to mpv fails.
    pub async fn play_queue_position_at(
        self: &Arc<Self>,
        pos: usize,
        mode: PlayMode,
        start_at: f64,
    ) -> Result<(), Error> {
        let _transition = self.playback_transitions.lock().await;
        self.play_queue_position_at_locked(pos, mode, start_at, PlayIntent::Start)
            .await
    }

    async fn play_queue_position_at_locked(
        self: &Arc<Self>,
        pos: usize,
        mode: PlayMode,
        start_at: f64,
        intent: PlayIntent,
    ) -> Result<(), Error> {
        let Some(client) = self.subsonic.read().await.clone() else {
            return Ok(());
        };

        let (song, stream_url) = {
            let mut state = self.state.write().await;
            let new_instance = matches!(intent, PlayIntent::Start);
            match self.commit_play_state_in_lock(&mut state, &client, pos, new_instance) {
                Ok(v) => v,
                Err(()) => return Ok(()),
            }
        };

        info!(
            "Playing: {} (queue pos {}) mode={:?} start={}",
            song.title, pos, mode, start_at
        );

        self.dispatch_play(stream_url, pos, mode, start_at).await?;
        if start_at > 0.0 {
            let mut state = self.state.write().await;
            state.now_playing.position = start_at;
        }
        self.emit_now_playing().await;
        self.emit_queue().await;
        Ok(())
    }

    /// Repeat-aware: loads current for One, wraps for All, no-ops at the end for Off.
    // significant_drop_tightening: tokio guard held to scope; not tightened (early-drop is borrow-blocked, spans a trailing await, or saves nothing before return).
    #[allow(clippy::significant_drop_tightening)]
    pub async fn preload_next_track(self: &Arc<Self>, current_pos: usize) {
        let gen = self.loadfile_gen.load(std::sync::atomic::Ordering::Acquire);
        let (next_song, cache_enabled) = {
            let state = self.state.read().await;
            let queue_len = state.queue.len();
            let target = state.config.repeat_mode.next_auto(current_pos, queue_len);
            match target.and_then(|p| state.queue.get(p)) {
                Some(s) => (s.clone(), state.config.offline_cache_enabled),
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
        // Gapless preload prefers a cached copy so an already-downloaded next
        // track never re-fetches.
        let url = if cache_enabled {
            self.cached_track_path(&next_song.id)
                .map_or(url, |path| path.to_string_lossy().into_owned())
        } else {
            url
        };

        let mut mpv = self.mpv.lock().await;
        // A newer loadfile (concurrent play or a tick advance) superseded this
        // preload's current track; skip so a stale next never lands in slot 1.
        if self.loadfile_gen.load(std::sync::atomic::Ordering::Acquire) != gen {
            debug!("preload superseded by a newer load; skipping append");
            return;
        }
        // Single-flight: append a next track only when the current track is the lone playlist entry; the mpv lock serializes racing preloads so a double-append (count 3) cannot desync the gapless advance.
        match mpv.get_playlist_count().await {
            Ok(1) => {}
            Ok(c) => {
                debug!("preload skip: playlist count {c} != 1 (next already loaded or idle)");
                return;
            }
            Err(e) => {
                debug!("preload skip: playlist count query failed: {e}");
                return;
            }
        }
        if let Err(e) = mpv.loadfile_append(&url).await {
            debug!("Failed to pre-load next track: {}", e);
        } else {
            debug!("Preload appended next track; playlist count now 2");
        }
    }

    /// Re-align mpv's preloaded next track with the current queue after a queue mutation, so a gapless advance plays the queue's next track and not a stale preload. No-op unless actively `Playing`; drops mpv's slot-1 preload and re-preloads the repeat-aware next.
    // significant_drop_tightening: tokio guard held to scope; not tightened (early-drop is borrow-blocked, spans a trailing await, or saves nothing before return).
    #[allow(clippy::significant_drop_tightening)]
    pub async fn resync_gapless_preload(self: &Arc<Self>) {
        use crate::daemon::state::PlaybackState;
        let pos = {
            let state = self.state.read().await;
            if state.now_playing.state != PlaybackState::Playing {
                return;
            }
            match state.queue_position {
                Some(p) => p,
                None => return,
            }
        };
        {
            let mut mpv = self.mpv.lock().await;
            if let Ok(count) = mpv.get_playlist_count().await {
                if count > 1 {
                    let _ = mpv.playlist_remove(1).await;
                }
            }
        }
        self.preload_next_track(pos).await;
    }

    /// End-of-queue stop: halt mpv, mark `Stopped`, emit, and release the `PipeWire` pin so the idle daemon stops holding the device at the last track's rate.
    // significant_drop_tightening: tokio guard held to scope; not tightened (early-drop is borrow-blocked, spans a trailing await, or saves nothing before return).
    #[allow(clippy::significant_drop_tightening)]
    async fn finish_at_queue_end(self: &Arc<Self>) {
        use crate::daemon::state::PlaybackState;
        info!("Reached end of queue");
        self.cancel_prebuffer().await;
        {
            let mut mpv = self.mpv.lock().await;
            let _ = mpv.stop().await;
        }
        {
            let mut state = self.state.write().await;
            state.now_playing.state = PlaybackState::Stopped;
            state.now_playing.position = 0.0;
        }
        self.emit_now_playing().await;
        self.release_pipewire_rate().await;
    }

    /// Stop playback, unload the track, and broadcast the state change.
    ///
    /// # Errors
    /// Returns an `Error` if mpv control or a server request fails.
    pub async fn stop_playback(self: &Arc<Self>) -> Result<(), Error> {
        use crate::daemon::state::PlaybackState;
        self.cancel_prebuffer().await;
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
        self.release_pipewire_rate().await;
        Ok(())
    }

    /// MPRIS / Stop-button semantics: halt playback but keep the queue and current selection intact so Play can resume the same track.
    ///
    /// # Errors
    /// Returns an `Error` if mpv control or a server request fails.
    pub async fn stop_keep_queue(self: &Arc<Self>) -> Result<(), Error> {
        use crate::daemon::state::PlaybackState;
        self.cancel_prebuffer().await;
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
        self.release_pipewire_rate().await;
        Ok(())
    }

    /// Stop mpv without touching the queue.
    pub async fn halt_keep_queue(self: &Arc<Self>) {
        use crate::daemon::state::PlaybackState;
        self.cancel_prebuffer().await;
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
        self.release_pipewire_rate().await;
    }

    /// Seek to an absolute position in seconds.
    ///
    /// # Errors
    /// Returns an `Error` if mpv control or a server request fails.
    // significant_drop_tightening: tokio guard held to scope; not tightened (early-drop is borrow-blocked, spans a trailing await, or saves nothing before return).
    #[allow(clippy::significant_drop_tightening)]
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

    /// Seek by a signed offset in seconds.
    ///
    /// # Errors
    /// Returns an `Error` if mpv control or a server request fails.
    // significant_drop_tightening: tokio guard held to scope; not tightened (early-drop is borrow-blocked, spans a trailing await, or saves nothing before return).
    #[allow(clippy::significant_drop_tightening)]
    pub async fn seek_relative(self: &Arc<Self>, offset: f64) -> Result<(), Error> {
        let mut mpv = self.mpv.lock().await;
        let _ = mpv.seek_relative(offset).await;
        Ok(())
    }

    /// Set mpv volume as a percentage.
    ///
    /// # Errors
    /// Returns an `Error` if mpv control or a server request fails.
    // significant_drop_tightening: tokio guard held to scope; not tightened (early-drop is borrow-blocked, spans a trailing await, or saves nothing before return).
    #[allow(clippy::significant_drop_tightening)]
    pub async fn set_volume(self: &Arc<Self>, vol: i32) -> Result<(), Error> {
        let clamped = vol.clamp(0, 100);
        let mut mpv = self.mpv.lock().await;
        let _ = mpv.set_volume(clamped).await;
        drop(mpv);
        self.state.write().await.now_playing.volume = clamped;
        self.emit_now_playing().await;
        Ok(())
    }
}
