//! Queue mutation: replace + play, move, clear-history, shuffle.

use std::sync::Arc;

use tracing::{error, info};

use crate::daemon::core::{DaemonCore, PlayMode};
use crate::error::Error;
use crate::ipc::protocol::DaemonEvent;

impl DaemonCore {
    /// Replace queue + play target under a single state write lock so the queue cannot be mutated between the swap and the play setup. If `play_from` is None, only the queue is replaced.
    pub async fn replace_queue_and_play(
        self: &Arc<Self>,
        songs: Vec<crate::subsonic::models::Child>,
        play_from: Option<usize>,
        mode: PlayMode,
    ) -> Result<(), Error> {
        let client_opt = self.subsonic.read().await.clone();

        let prepared = {
            let mut state = self.state.write().await;
            state.queue = songs;
            state.queue_position = None;
            match (play_from, client_opt) {
                (Some(idx), Some(client)) => self
                    .commit_play_state_in_lock(&mut state, &client, idx)
                    .ok()
                    .map(|(s, u)| (s, u, idx)),
                _ => None,
            }
        };

        self.broadcast_queue_changed().await;

        let Some((song, stream_url, idx)) = prepared else {
            return Ok(());
        };

        info!(
            "Playing: {} (queue pos {}) mode={:?}",
            song.title, idx, mode
        );

        self.dispatch_play(stream_url, idx, mode).await?;
        self.emit_now_playing().await;
        self.emit_queue().await;
        self.spawn_fast_probe();
        Ok(())
    }

    /// Push the current queue to all subscribers.
    pub async fn broadcast_queue_changed(self: &Arc<Self>) {
        self.emit_queue().await;
    }

    /// Reorder; `queue_position` is adjusted to keep pointing at the same song.
    pub async fn move_queue_item(self: &Arc<Self>, from: usize, to: usize) {
        let mut state = self.state.write().await;
        let len = state.queue.len();
        if from >= len || to >= len || from == to {
            return;
        }
        let song = state.queue.remove(from);
        state.queue.insert(to, song);
        if let Some(cur) = state.queue_position {
            let new_cur = if cur == from {
                to
            } else if from < cur && to >= cur {
                cur - 1
            } else if from > cur && to <= cur {
                cur + 1
            } else {
                cur
            };
            state.queue_position = Some(new_cur);
        }
        drop(state);
        self.emit_queue().await;
        self.resync_gapless_preload().await;
    }

    /// Drain entries before `queue_position`. Returns count removed.
    pub async fn clear_queue_history(self: &Arc<Self>) -> usize {
        let mut state = self.state.write().await;
        let Some(pos) = state.queue_position else {
            return 0;
        };
        if pos == 0 {
            return 0;
        }
        let removed = pos;
        state.queue.drain(0..pos);
        state.queue_position = Some(0);
        drop(state);
        self.emit_queue().await;
        removed
    }

    /// Replace the queue with a shuffled random-songs batch and start playing.
    pub async fn shuffle_library(self: &Arc<Self>) -> Result<(), Error> {
        let Some(client) = self.subsonic.read().await.clone() else {
            return Ok(());
        };
        let songs = match client.get_random_songs().await {
            Ok(s) if !s.is_empty() => s,
            Ok(_) => return Ok(()),
            Err(e) => {
                error!("Failed to load random songs: {}", e);
                self.emit(DaemonEvent::Notification {
                    message: format!("Failed to shuffle library: {}", e),
                    is_error: true,
                });
                return Ok(());
            }
        };
        {
            let mut state = self.state.write().await;
            state.library.random_songs = songs.clone();
            state.queue = songs.clone();
            state.queue_position = None;
        }
        self.emit(DaemonEvent::RandomChanged(songs));
        self.emit_queue().await;
        self.play_queue_position(0, PlayMode::Buffered).await
    }

    /// Shuffle preserving the currently-playing track in place.
    pub async fn shuffle_queue(self: &Arc<Self>) {
        use rand::seq::SliceRandom;
        // Scope `thread_rng` (!Send) out of the await below.
        {
            let mut state = self.state.write().await;
            if state.queue.is_empty() {
                return;
            }
            let mut rng = rand::thread_rng();
            match state.queue_position {
                Some(cur) if cur < state.queue.len() => {
                    let current = state.queue.remove(cur);
                    state.queue.shuffle(&mut rng);
                    state.queue.insert(cur, current);
                }
                _ => state.queue.shuffle(&mut rng),
            }
        }
        self.emit_queue().await;
        self.resync_gapless_preload().await;
    }
}
