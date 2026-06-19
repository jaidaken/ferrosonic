//! Playback control: pause/resume, seek, skip, advance, preload, stop, volume.

use std::sync::Arc;

use tracing::{debug, error, info, warn};

use crate::daemon::core::{DaemonCore, PlayMode};
use crate::error::Error;

impl DaemonCore {
    /// Toggle pause by current state: `Playing` pauses, `Paused` resumes, `Stopped` with a queued position starts playback. Delegates so the PipeWire pin release/re-apply lives in one place per direction.
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

    /// Pause playback. Stops mpv so it disconnects its PipeWire stream; the
    /// playhead is kept in `now_playing.position` and resume reloads + seeks
    /// back. The force-rate pin is deliberately kept (released only on stop)
    /// so resuming the same track needs no device re-clock and stays gapless.
    /// Commits `Paused` before the stop so the idle tick (gated on `is_playing`)
    /// cannot read the stop as a track-end and auto-advance.
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
        {
            let mut mpv = self.mpv.lock().await;
            if let Err(e) = mpv.stop().await {
                error!("Failed to stop mpv on pause: {}", e);
            }
        }
        self.emit_now_playing().await;
        Ok(())
    }

    /// Resume from pause by reloading the current track and seeking back to the saved position (mpv was stopped on pause to free the audio device), which re-pins the rate via the normal play path. From `Stopped` with a queued position, starts that track from the top.
    pub async fn resume_playback(self: &Arc<Self>) -> Result<(), Error> {
        use crate::daemon::state::PlaybackState;
        let (playback_state, queue_pos, resume_at) = {
            let state = self.state.read().await;
            (
                state.now_playing.state,
                state.queue_position,
                state.now_playing.position,
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
        self.play_queue_position_at(pos, PlayMode::Direct, start_at)
            .await?;
        Ok(())
    }

    /// Manual skip. Ignores `repeat=One` (user wants to move).
    pub async fn next_track(self: &Arc<Self>) -> Result<(), Error> {
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
        self.finish_at_queue_end().await;
        Ok(())
    }

    /// Auto-end advance. Honours `repeat=One` and `repeat=All`.
    pub async fn advance_auto(self: &Arc<Self>) -> Result<(), Error> {
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
        self.finish_at_queue_end().await;
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

    /// Load and play the queue entry at `pos` from the start; drives the `PipeWire` rate switch.
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

    /// Re-align mpv's preloaded next track with the current queue after a queue mutation, so a gapless advance plays the queue's next track and not a stale preload. No-op unless actively `Playing`; drops mpv's slot-1 preload and re-preloads the repeat-aware next.
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

    /// End-of-queue stop: halt mpv, mark `Stopped`, emit, and release the PipeWire pin so the idle daemon stops holding the device at the last track's rate.
    async fn finish_at_queue_end(self: &Arc<Self>) {
        use crate::daemon::state::PlaybackState;
        info!("Reached end of queue");
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
    pub async fn stop_playback(self: &Arc<Self>) -> Result<(), Error> {
        use crate::daemon::state::PlaybackState;
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
    pub async fn stop_keep_queue(self: &Arc<Self>) -> Result<(), Error> {
        use crate::daemon::state::PlaybackState;
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
    pub async fn seek_relative(self: &Arc<Self>, offset: f64) -> Result<(), Error> {
        let mut mpv = self.mpv.lock().await;
        let _ = mpv.seek_relative(offset).await;
        Ok(())
    }

    /// Set mpv volume as a percentage.
    pub async fn set_volume(self: &Arc<Self>, vol: i32) -> Result<(), Error> {
        let mut mpv = self.mpv.lock().await;
        let _ = mpv.set_volume(vol).await;
        Ok(())
    }
}
