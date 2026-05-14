//! Playback control: pause/resume, seek, skip, advance, preload, stop, volume.

use std::sync::Arc;

use tracing::{debug, error, info, warn};

use crate::daemon::core::{DaemonCore, PlayMode};
use crate::error::Error;

impl DaemonCore {
    pub async fn toggle_pause(self: &Arc<Self>) -> Result<(), Error> {
        use crate::daemon::state::PlaybackState;
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
                // R1+R2: re-check state under the write lock; a concurrent Stop between the initial read and the mpv ack must not be overwritten by the pause toggle.
                let updated = {
                    let mut state = self.state.write().await;
                    let cur = state.now_playing.state;
                    if cur != PlaybackState::Playing && cur != PlaybackState::Paused {
                        false
                    } else {
                        state.now_playing.state = if now_paused {
                            PlaybackState::Paused
                        } else {
                            PlaybackState::Playing
                        };
                        debug!("toggle_pause: now {:?}", state.now_playing.state);
                        true
                    }
                };
                if updated {
                    self.emit_now_playing().await;
                }
            }
            Err(e) => {
                error!("Failed to toggle pause: {}", e);
            }
        }
        Ok(())
    }

    pub async fn pause_playback(self: &Arc<Self>) -> Result<(), Error> {
        use crate::daemon::state::PlaybackState;
        // R1: take the write lock upfront so the Playing check and the eventual Paused commit cover one consistent snapshot.
        let mut state = self.state.write().await;
        if state.now_playing.state != PlaybackState::Playing {
            return Ok(());
        }
        drop(state);
        let mut mpv = self.mpv.lock().await;
        match mpv.pause().await {
            Ok(()) => {
                drop(mpv);
                state = self.state.write().await;
                if state.now_playing.state != PlaybackState::Playing {
                    return Ok(());
                }
                state.now_playing.state = PlaybackState::Paused;
                drop(state);
                self.emit_now_playing().await;
            }
            Err(e) => error!("Failed to pause: {}", e),
        }
        Ok(())
    }

    pub async fn resume_playback(self: &Arc<Self>) -> Result<(), Error> {
        use crate::daemon::state::PlaybackState;
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
        use crate::daemon::state::PlaybackState;
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

    /// Auto-end advance. Honours `repeat=One` and `repeat=All`.
    pub async fn advance_auto(self: &Arc<Self>) -> Result<(), Error> {
        use crate::daemon::state::PlaybackState;
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
}
