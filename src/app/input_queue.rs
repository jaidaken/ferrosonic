use crossterm::event::{self, KeyCode};

use crate::error::Error;

use super::*;

impl App {
    pub(super) async fn handle_queue_key(&mut self, key: event::KeyEvent) -> Result<(), Error> {
        let ds = self.daemon_state.read().await;
        let mut cs = self.client_state.write().await;
        let state = AppState {
            daemon: &ds,
            client: &mut cs,
        };

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
                if let Some(idx) = state.client.queue_state.selected {
                    if idx < state.daemon.queue.len() {
                        drop(state);
                        drop(cs);
                        drop(ds);
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
                if let Some(idx) = state.client.queue_state.selected {
                    if idx < state.daemon.queue.len() {
                        let removed_title = state
                            .daemon
                            .queue
                            .get(idx)
                            .map(|s| s.title.clone())
                            .unwrap_or_default();
                        let queue_len = state.daemon.queue.len();
                        if queue_len <= 1 {
                            state.client.queue_state.selected = None;
                        } else if idx >= queue_len - 1 {
                            state.client.queue_state.selected = Some(queue_len - 2);
                        }
                        state.client.notify(format!("Removed: {}", removed_title));
                        drop(state);
                        drop(cs);
                        drop(ds);
                        let _ = self
                            .client
                            .request(DaemonRequest::RemoveFromQueue(idx))
                            .await;
                        return Ok(());
                    }
                }
            }
            KeyCode::Char('J') => {
                if let Some(idx) = state.client.queue_state.selected {
                    if idx + 1 < state.daemon.queue.len() {
                        state.client.queue_state.selected = Some(idx + 1);
                        drop(state);
                        drop(cs);
                        drop(ds);
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
                if let Some(idx) = state.client.queue_state.selected {
                    if idx > 0 {
                        state.client.queue_state.selected = Some(idx - 1);
                        drop(state);
                        drop(cs);
                        drop(ds);
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
            KeyCode::Char('t') => {
                state.client.notify("Queue shuffled");
                drop(state);
                drop(cs);
                drop(ds);
                let _ = self.client.request(DaemonRequest::ShuffleQueue).await;
                return Ok(());
            }
            KeyCode::Char('c') => {
                let pos = state.daemon.queue_position;
                let sel_before = state.client.queue_state.selected;
                drop(state);
                drop(cs);
                drop(ds);
                if let Ok(crate::ipc::DaemonResponse::HistoryCleared(removed)) =
                    self.client.request(DaemonRequest::ClearQueueHistory).await
                {
                    let ds = self.daemon_state.read().await;
                    let mut cs = self.client_state.write().await;
                    let state = AppState {
                        daemon: &ds,
                        client: &mut cs,
                    };
                    if removed == 0 {
                        state.client.notify("No history to clear");
                    } else {
                        state
                            .client
                            .notify(format!("Cleared {} played songs", removed));
                        // Re-anchor client selection to the same song post-trim.
                        if let (Some(p), Some(sel)) = (pos, sel_before) {
                            state.client.queue_state.selected = Some(sel.saturating_sub(p));
                        }
                    }
                }
                return Ok(());
            }
            KeyCode::Char('m') => {
                let song_id = state
                    .client
                    .queue_state
                    .selected
                    .and_then(|idx| state.daemon.queue.get(idx).map(|s| s.id.clone()));
                drop(state);
                drop(cs);
                drop(ds);
                if let Some(id) = song_id {
                    let _ = self.client.request(DaemonRequest::ToggleStarSong(id)).await;
                }
                return Ok(());
            }
            _ => {}
        }

        Ok(())
    }
}
