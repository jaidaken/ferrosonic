use crossterm::event::{self, KeyCode};

use crate::error::Error;

use super::*;

impl App {
    /// Handle queue page keys
    pub(super) async fn handle_queue_key(&mut self, key: event::KeyEvent) -> Result<(), Error> {
        let mut state = self.state.write().await;

        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                if let Some(sel) = state.client.queue_state.selected {
                    if sel > 0 {
                        state.client.queue_state.selected = Some(sel - 1);
                    }
                } else if !state.daemon.queue.is_empty() {
                    state.client.queue_state.selected = Some(0);
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                let max = state.daemon.queue.len().saturating_sub(1);
                if let Some(sel) = state.client.queue_state.selected {
                    if sel < max {
                        state.client.queue_state.selected = Some(sel + 1);
                    }
                } else if !state.daemon.queue.is_empty() {
                    state.client.queue_state.selected = Some(0);
                }
            }
            KeyCode::Enter => {
                // Play selected song
                if let Some(idx) = state.client.queue_state.selected {
                    if idx < state.daemon.queue.len() {
                        drop(state);
                        return self
                            .client
                            .request(DaemonRequest::PlayQueueIndex(idx))
                            .await
                            .map(|_| ())
                            .map_err(Error::from);
                    }
                }
            }
            KeyCode::Char('d') => {
                // Remove selected song. Daemon adjusts queue_position;
                // client adjusts its own selection (a UI concern).
                if let Some(idx) = state.client.queue_state.selected {
                    if idx < state.daemon.queue.len() {
                        let removed_title = state
                            .daemon
                            .queue
                            .get(idx)
                            .map(|s| s.title.clone())
                            .unwrap_or_default();
                        let queue_len = state.daemon.queue.len();
                        // Adjust client-side selection toward the new
                        // post-remove position.
                        if queue_len <= 1 {
                            state.client.queue_state.selected = None;
                        } else if idx >= queue_len - 1 {
                            state.client.queue_state.selected = Some(queue_len - 2);
                        }
                        state.client.notify(format!("Removed: {}", removed_title));
                        drop(state);
                        let _ = self
                            .client
                            .request(DaemonRequest::RemoveFromQueue(idx))
                            .await;
                        return Ok(());
                    }
                }
            }
            KeyCode::Char('J') => {
                // Move down
                if let Some(idx) = state.client.queue_state.selected {
                    if idx + 1 < state.daemon.queue.len() {
                        state.client.queue_state.selected = Some(idx + 1);
                        drop(state);
                        let _ = self
                            .client
                            .request(DaemonRequest::MoveQueueItem {
                                from: idx,
                                to: idx + 1,
                            })
                            .await;
                        return Ok(());
                    }
                }
            }
            KeyCode::Char('K') => {
                // Move up
                if let Some(idx) = state.client.queue_state.selected {
                    if idx > 0 {
                        state.client.queue_state.selected = Some(idx - 1);
                        drop(state);
                        let _ = self
                            .client
                            .request(DaemonRequest::MoveQueueItem {
                                from: idx,
                                to: idx - 1,
                            })
                            .await;
                        return Ok(());
                    }
                }
            }
            KeyCode::Char('r') => {
                // Shuffle queue
                state.client.notify("Queue shuffled");
                drop(state);
                let _ = self.client.request(DaemonRequest::ShuffleQueue).await;
                return Ok(());
            }
            KeyCode::Char('c') => {
                // Clear history (drain entries before queue_position)
                let pos = state.daemon.queue_position;
                let sel_before = state.client.queue_state.selected;
                drop(state);
                match self
                    .client
                    .request(DaemonRequest::ClearQueueHistory)
                    .await
                {
                    Ok(crate::ipc::DaemonResponse::HistoryCleared(removed)) => {
                        let mut state = self.state.write().await;
                        if removed == 0 {
                            state.client.notify("No history to clear");
                        } else {
                            state.client.notify(format!("Cleared {} played songs", removed));
                            // Fix up client selection so it points at
                            // the same song relative to the trimmed queue.
                            if let (Some(p), Some(sel)) = (pos, sel_before) {
                                state.client.queue_state.selected =
                                    Some(if sel < p { 0 } else { sel - p });
                            }
                        }
                    }
                    Ok(_) | Err(_) => {}
                }
                return Ok(());
            }
            _ => {}
        }

        Ok(())
    }
}
