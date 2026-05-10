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
                        return self.client.request(DaemonRequest::PlayQueueIndex(idx)).await.map(|_| ()).map_err(Error::from);
                    }
                }
            }
            KeyCode::Char('d') => {
                // Remove selected song
                if let Some(idx) = state.client.queue_state.selected {
                    if idx < state.daemon.queue.len() {
                        let song = state.daemon.queue.remove(idx);
                        state.client.notify(format!("Removed: {}", song.title));
                        // Adjust selection
                        if state.daemon.queue.is_empty() {
                            state.client.queue_state.selected = None;
                        } else if idx >= state.daemon.queue.len() {
                            state.client.queue_state.selected = Some(state.daemon.queue.len() - 1);
                        }
                        // Adjust queue position
                        if let Some(pos) = state.daemon.queue_position {
                            if idx < pos {
                                state.daemon.queue_position = Some(pos - 1);
                            } else if idx == pos {
                                state.daemon.queue_position = None;
                            }
                        }
                    }
                }
            }
            KeyCode::Char('J') => {
                // Move down
                if let Some(idx) = state.client.queue_state.selected {
                    if idx < state.daemon.queue.len() - 1 {
                        state.daemon.queue.swap(idx, idx + 1);
                        state.client.queue_state.selected = Some(idx + 1);
                        // Adjust queue position if needed
                        if let Some(pos) = state.daemon.queue_position {
                            if pos == idx {
                                state.daemon.queue_position = Some(idx + 1);
                            } else if pos == idx + 1 {
                                state.daemon.queue_position = Some(idx);
                            }
                        }
                    }
                }
            }
            KeyCode::Char('K') => {
                // Move up
                if let Some(idx) = state.client.queue_state.selected {
                    if idx > 0 {
                        state.daemon.queue.swap(idx, idx - 1);
                        state.client.queue_state.selected = Some(idx - 1);
                        // Adjust queue position if needed
                        if let Some(pos) = state.daemon.queue_position {
                            if pos == idx {
                                state.daemon.queue_position = Some(idx - 1);
                            } else if pos == idx - 1 {
                                state.daemon.queue_position = Some(idx);
                            }
                        }
                    }
                }
            }
            KeyCode::Char('r') => {
                // Shuffle queue
                use rand::seq::SliceRandom;
                let mut rng = rand::thread_rng();

                if let Some(pos) = state.daemon.queue_position {
                    // Keep current song in place, shuffle the rest
                    if pos < state.daemon.queue.len() {
                        let current = state.daemon.queue.remove(pos);
                        state.daemon.queue.shuffle(&mut rng);
                        state.daemon.queue.insert(0, current);
                        state.daemon.queue_position = Some(0);
                    }
                } else {
                    state.daemon.queue.shuffle(&mut rng);
                }
                state.client.notify("Queue shuffled");
            }
            KeyCode::Char('c') => {
                // Clear history (remove all songs before current position)
                if let Some(pos) = state.daemon.queue_position {
                    if pos > 0 {
                        let removed = pos;
                        state.daemon.queue.drain(0..pos);
                        state.daemon.queue_position = Some(0);
                        // Adjust selection
                        if let Some(sel) = state.client.queue_state.selected {
                            if sel < pos {
                                state.client.queue_state.selected = Some(0);
                            } else {
                                state.client.queue_state.selected = Some(sel - pos);
                            }
                        }
                        state.client.notify(format!("Cleared {} played songs", removed));
                    } else {
                        state.client.notify("No history to clear");
                    }
                } else {
                    state.client.notify("No history to clear");
                }
            }
            _ => {}
        }

        Ok(())
    }
}
