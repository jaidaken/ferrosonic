use tracing::{debug, error, info, warn};

use super::*;

impl App {
    /// Update playback position and audio info from MPV
    pub(super) async fn update_playback_info(&mut self) {
        // Only update if something should be playing
        let state = self.state.read().await;
        let is_playing = state.now_playing.state == PlaybackState::Playing;
        let is_active = is_playing || state.now_playing.state == PlaybackState::Paused;
        drop(state);

        if !is_active || !self.mpv.is_running() {
            return;
        }

        // Check for track advancement
        if is_playing {
            // Early transition: if near end of track and no preloaded next track,
            // advance immediately instead of waiting for idle detection
            {
                let state = self.state.read().await;
                let time_remaining = state.now_playing.duration - state.now_playing.position;
                let has_next = state
                    .queue_position
                    .map(|p| p + 1 < state.queue.len())
                    .unwrap_or(false);
                drop(state);

                if has_next && time_remaining > 0.0 && time_remaining < 2.0 {
                    if let Ok(count) = self.mpv.get_playlist_count() {
                        if count < 2 {
                            info!("Near end of track with no preloaded next — advancing early");
                            let _ = self.next_track().await;
                            return;
                        }
                    }
                }
            }

            // Re-preload if the appended track was lost
            if let Ok(count) = self.mpv.get_playlist_count() {
                if count == 1 {
                    let state = self.state.read().await;
                    if let Some(pos) = state.queue_position {
                        if pos + 1 < state.queue.len() {
                            drop(state);
                            debug!("Playlist count is 1, re-preloading next track");
                            self.preload_next_track(pos).await;
                        }
                    }
                }
            }

            // Check if MPV advanced to next track in playlist (gapless transition)
            if let Ok(Some(mpv_pos)) = self.mpv.get_playlist_pos() {
                if mpv_pos == 1 {
                    // Gapless advance happened - update our state to match
                    let state = self.state.read().await;
                    if let Some(current_pos) = state.queue_position {
                        let next_pos = current_pos + 1;
                        if next_pos < state.queue.len() {
                            drop(state);
                            info!("Gapless advancement to track {}", next_pos);

                            // Update state - keep audio properties since they'll be similar
                            // for gapless transitions (same album, same format)
                            let mut state = self.state.write().await;
                            state.queue_position = Some(next_pos);
                            if let Some(song) = state.queue.get(next_pos).cloned() {
                                state.now_playing.song = Some(song.clone());
                                state.now_playing.position = 0.0;
                                state.now_playing.duration = song.duration.unwrap_or(0) as f64;
                                state.now_playing.marked_played = false;
                                state.now_playing.marked_playing = false;
                                // Don't reset audio properties - let them update naturally
                                // This avoids triggering PipeWire rate changes unnecessarily
                            }
                            drop(state);

                            // Remove the finished track (index 0) from MPV's playlist
                            // This is less disruptive than playlist_clear during playback
                            let _ = self.mpv.playlist_remove(0);

                            // Preload the next track for continued gapless playback
                            self.preload_next_track(next_pos).await;
                            return;
                        }
                    }
                    drop(state);
                }
            }

            // Check if MPV went idle (track ended, no preloaded track)
            if let Ok(idle) = self.mpv.is_idle() {
                if idle {
                    info!("Track ended, advancing to next");
                    let _ = self.next_track().await;
                    return;
                }
            }
        }

        // Get position from MPV
        if let Ok(position) = self.mpv.get_time_pos() {
            let mut state = self.state.write().await;
            state.now_playing.position = position;
        }

        // Get duration if not set
        {
            let state = self.state.read().await;
            if state.now_playing.duration <= 0.0 {
                drop(state);
                if let Ok(duration) = self.mpv.get_duration() {
                    if duration > 0.0 {
                        let mut state = self.state.write().await;
                        state.now_playing.duration = duration;
                    }
                }
            }
        }

        {
            if let Some(client) = &self.subsonic {
                let state = self.state.read().await;
                if state.now_playing.state == PlaybackState::Playing {
                    if let Some(song) = state.now_playing.song.clone() {
                        // If the currently playing song has not been marked as so yet, do so.
                        if !state.now_playing.marked_playing && state.now_playing.duration > 0.0 {
                            let song_id = song.id.clone();
                            drop(state);

                            {
                                let mut state = self.state.write().await;
                                state.now_playing.marked_playing = true;
                            }

                            let client = client.clone();
                            tokio::spawn(async move {
                                if let Err(e) = client.scrobble(&song_id, false).await {
                                    error!("Failed to scrobble: {}", e);
                                } else {
                                    info!("Successfully scrobbled track {}", song_id);
                                }});
                        } else {
                            drop(state);
                        }

                        let state = self.state.read().await;
                        // If the currently playing song is > 70% done, mark it as played
                        if !state.now_playing.marked_played && state.now_playing.duration > 0.0 && state.now_playing.progress_percent() > 0.7 {

                            let song_id = song.id.clone();
                            drop(state);

                            {
                                let mut state = self.state.write().await;
                                state.now_playing.marked_played = true;
                            }

                            let client = client.clone();
                            tokio::spawn(async move {
                                if let Err(e) = client.scrobble(&song_id, true).await {
                                    error!("Failed to scrobble: {}", e);
                                } else {
                                    info!("Successfully scrobbled track {}", song_id);
                                }});
                        }
                    }
                }
            }
        }

        // Get audio properties - keep polling until we get valid values
        // MPV may not have them ready immediately when playback starts
        {
            let state = self.state.read().await;
            let need_sample_rate = state.now_playing.sample_rate.is_none();
            drop(state);

            if need_sample_rate {
                // Try to get audio properties from MPV
                let sample_rate = self.mpv.get_sample_rate().ok().flatten();
                let bit_depth = self.mpv.get_bit_depth().ok().flatten();
                let format = self.mpv.get_audio_format().ok().flatten();
                let channels = self.mpv.get_channels().ok().flatten();

                // Only update if we got a valid sample rate (indicates audio is ready)
                if let Some(rate) = sample_rate {
                    // Only switch PipeWire sample rate if it's actually different
                    // This avoids unnecessary rate switches during gapless playback
                    // of albums with the same sample rate
                    let current_pw_rate = self.pipewire.get_current_rate();
                    if current_pw_rate != Some(rate) {
                        info!("Sample rate change: {:?} -> {} Hz", current_pw_rate, rate);
                        if let Err(e) = self.pipewire.set_rate(rate) {
                            warn!("Failed to set PipeWire sample rate: {}", e);
                        }
                    } else {
                        debug!(
                            "Sample rate unchanged at {} Hz, skipping PipeWire switch",
                            rate
                        );
                    }

                    let mut state = self.state.write().await;
                    state.now_playing.sample_rate = Some(rate);
                    state.now_playing.bit_depth = bit_depth;
                    state.now_playing.format = format;
                    state.now_playing.channels = channels;
                }
            }
        }

        // Update MPRIS properties to keep external clients in sync
        if let Some(ref server) = self.mpris_server {
            if let Err(e) = update_mpris_properties(server, &self.state).await {
                debug!("Failed to update MPRIS properties: {}", e);
            }
        }
    }

    /// Toggle play/pause
    pub(super) async fn toggle_pause(&mut self) -> Result<(), Error> {
        let state = self.state.read().await;
        let is_playing = state.now_playing.state == PlaybackState::Playing;
        let is_paused = state.now_playing.state == PlaybackState::Paused;
        drop(state);

        if !is_playing && !is_paused {
            return Ok(());
        }

        match self.mpv.toggle_pause() {
            Ok(now_paused) => {
                let mut state = self.state.write().await;
                if now_paused {
                    state.now_playing.state = PlaybackState::Paused;
                    debug!("Paused playback");
                } else {
                    state.now_playing.state = PlaybackState::Playing;
                    debug!("Resumed playback");
                }
            }
            Err(e) => {
                error!("Failed to toggle pause: {}", e);
            }
        }
        Ok(())
    }

    /// Pause playback (only if currently playing)
    pub(super) async fn pause_playback(&mut self) -> Result<(), Error> {
        let state = self.state.read().await;
        if state.now_playing.state != PlaybackState::Playing {
            return Ok(());
        }
        drop(state);

        match self.mpv.pause() {
            Ok(()) => {
                let mut state = self.state.write().await;
                state.now_playing.state = PlaybackState::Paused;
                debug!("Paused playback");
            }
            Err(e) => {
                error!("Failed to pause: {}", e);
            }
        }
        Ok(())
    }

    /// Resume playback (only if currently paused)
    pub(super) async fn resume_playback(&mut self) -> Result<(), Error> {
        let state = self.state.read().await;
        if state.now_playing.state != PlaybackState::Paused {
            return Ok(());
        }
        drop(state);

        match self.mpv.resume() {
            Ok(()) => {
                let mut state = self.state.write().await;
                state.now_playing.state = PlaybackState::Playing;
                debug!("Resumed playback");
            }
            Err(e) => {
                error!("Failed to resume: {}", e);
            }
        }
        Ok(())
    }

    /// Play next track in queue
    pub(super) async fn next_track(&mut self) -> Result<(), Error> {
        let state = self.state.read().await;
        let queue_len = state.queue.len();
        let current_pos = state.queue_position;
        drop(state);

        if queue_len == 0 {
            return Ok(());
        }

        let next_pos = match current_pos {
            Some(pos) if pos + 1 < queue_len => pos + 1,
            _ => {
                info!("Reached end of queue");
                let _ = self.mpv.stop();
                let mut state = self.state.write().await;
                state.now_playing.state = PlaybackState::Stopped;
                state.now_playing.position = 0.0;
                return Ok(());
            }
        };

        self.play_queue_position(next_pos).await
    }

    /// Play previous track in queue (or restart current if < 3 seconds in)
    pub(super) async fn prev_track(&mut self) -> Result<(), Error> {
        let state = self.state.read().await;
        let queue_len = state.queue.len();
        let current_pos = state.queue_position;
        let position = state.now_playing.position;
        drop(state);

        if queue_len == 0 {
            return Ok(());
        }

        if position < 3.0 {
            if let Some(pos) = current_pos {
                if pos > 0 {
                    return self.play_queue_position(pos - 1).await;
                }
            }
            if let Err(e) = self.mpv.seek(0.0) {
                error!("Failed to restart track: {}", e);
            } else {
                let mut state = self.state.write().await;
                state.now_playing.position = 0.0;
            }
            return Ok(());
        }

        debug!("Restarting current track (position: {:.1}s)", position);
        if let Err(e) = self.mpv.seek(0.0) {
            error!("Failed to restart track: {}", e);
        } else {
            let mut state = self.state.write().await;
            state.now_playing.position = 0.0;
        }
        Ok(())
    }

    /// Play a specific position in the queue
    pub(super) async fn play_queue_position(&mut self, pos: usize) -> Result<(), Error> {
        let state = self.state.read().await;
        let song = match state.queue.get(pos) {
            Some(s) => s.clone(),
            None => return Ok(()),
        };
        drop(state);

        let stream_url = if let Some(ref client) = self.subsonic {
            match client.get_stream_url(&song.id) {
                Ok(url) => url,
                Err(e) => {
                    error!("Failed to get stream URL: {}", e);
                    let mut state = self.state.write().await;
                    state.notify_error(format!("Failed to get stream URL: {}", e));
                    return Ok(());
                }
            }
        } else {
            return Ok(());
        };

        {
            let mut state = self.state.write().await;
            state.queue_position = Some(pos);
            state.now_playing.song = Some(song.clone());
            state.now_playing.state = PlaybackState::Playing;
            state.now_playing.position = 0.0;
            state.now_playing.duration = song.duration.unwrap_or(0) as f64;
            state.now_playing.sample_rate = None;
            state.now_playing.bit_depth = None;
            state.now_playing.format = None;
            state.now_playing.channels = None;
            state.now_playing.marked_playing = false;
            state.now_playing.marked_played = false;
        }

        info!("Playing: {} (queue pos {})", song.title, pos);
        if self.mpv.is_paused().unwrap_or(false) {
            let _ = self.mpv.resume();
        }
        if let Err(e) = self.mpv.loadfile(&stream_url) {
            error!("Failed to play: {}", e);
            let mut state = self.state.write().await;
            state.notify_error(format!("MPV error: {}", e));
            return Ok(());
        }

        self.preload_next_track(pos).await;

        Ok(())
    }

    /// Pre-load the next track into MPV's playlist for gapless playback
    pub(super) async fn preload_next_track(&mut self, current_pos: usize) {
        let state = self.state.read().await;
        let next_pos = current_pos + 1;

        if next_pos >= state.queue.len() {
            return;
        }

        let next_song = match state.queue.get(next_pos) {
            Some(s) => s.clone(),
            None => return,
        };
        drop(state);

        if let Some(ref client) = self.subsonic {
            if let Ok(url) = client.get_stream_url(&next_song.id) {
                debug!("Pre-loading next track for gapless: {}", next_song.title);
                if let Err(e) = self.mpv.loadfile_append(&url) {
                    debug!("Failed to pre-load next track: {}", e);
                } else if let Ok(count) = self.mpv.get_playlist_count() {
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
        }
    }

    /// Stop playback and clear the queue
    pub(super) async fn stop_playback(&mut self) -> Result<(), Error> {
        if let Err(e) = self.mpv.stop() {
            error!("Failed to stop: {}", e);
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
        state.now_playing.marked_playing = false;
        state.now_playing.marked_played = false;
        Ok(())
    }
}
