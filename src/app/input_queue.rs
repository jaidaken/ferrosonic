use crossterm::event::{self, KeyCode};

use crate::error::Error;

use super::*;

impl App {
    /// Handle queue page keys
    pub(super) async fn handle_queue_key(&mut self, key: event::KeyEvent) -> Result<(), Error> {
        let mut state = self.state.write().await;

        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                if let Some(sel) = state.queue_state.selected {
                    if sel > 0 {
                        state.queue_state.selected = Some(sel - 1);
                    }
                } else if !state.queue.is_empty() {
                    state.queue_state.selected = Some(0);
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                let max = state.queue.len().saturating_sub(1);
                if let Some(sel) = state.queue_state.selected {
                    if sel < max {
                        state.queue_state.selected = Some(sel + 1);
                    }
                } else if !state.queue.is_empty() {
                    state.queue_state.selected = Some(0);
                }
            }
            KeyCode::Enter => {
                // Play selected song
                if let Some(idx) = state.queue_state.selected {
                    if idx < state.queue.len() {
                        drop(state);
                        return self.play_queue_position(idx).await;
                    }
                }
            }
            KeyCode::Char('d') => {
                // Remove selected song
                if let Some(idx) = state.queue_state.selected {
                    if idx < state.queue.len() {
                        let song = state.queue.remove(idx);
                        state.notify(format!("Removed: {}", song.title));
                        // Adjust selection
                        if state.queue.is_empty() {
                            state.queue_state.selected = None;
                        } else if idx >= state.queue.len() {
                            state.queue_state.selected = Some(state.queue.len() - 1);
                        }
                        // Adjust queue position
                        if let Some(pos) = state.queue_position {
                            if idx < pos {
                                state.queue_position = Some(pos - 1);
                            } else if idx == pos {
                                state.queue_position = None;
                            }
                        }
                    }
                }
            }
            KeyCode::Char('J') => {
                // Move down
                if let Some(idx) = state.queue_state.selected {
                    if idx < state.queue.len() - 1 {
                        state.queue.swap(idx, idx + 1);
                        state.queue_state.selected = Some(idx + 1);
                        // Adjust queue position if needed
                        if let Some(pos) = state.queue_position {
                            if pos == idx {
                                state.queue_position = Some(idx + 1);
                            } else if pos == idx + 1 {
                                state.queue_position = Some(idx);
                            }
                        }
                    }
                }
            }
            KeyCode::Char('K') => {
                // Move up
                if let Some(idx) = state.queue_state.selected {
                    if idx > 0 {
                        state.queue.swap(idx, idx - 1);
                        state.queue_state.selected = Some(idx - 1);
                        // Adjust queue position if needed
                        if let Some(pos) = state.queue_position {
                            if pos == idx {
                                state.queue_position = Some(idx - 1);
                            } else if pos == idx - 1 {
                                state.queue_position = Some(idx);
                            }
                        }
                    }
                }
            }
            KeyCode::Char('s') => {
                // Shuffle queue
                use rand::seq::SliceRandom;
                let mut rng = rand::thread_rng();

                if let Some(pos) = state.queue_position {
                    // Keep current song in place, shuffle the rest
                    if pos < state.queue.len() {
                        let current = state.queue.remove(pos);
                        state.queue.shuffle(&mut rng);
                        state.queue.insert(0, current);
                        state.queue_position = Some(0);
                    }
                } else {
                    state.queue.shuffle(&mut rng);
                }
                state.notify("Queue shuffled");
            }
            KeyCode::Char('c') => {
                // Clear history (remove all songs before current position)
                if let Some(pos) = state.queue_position {
                    if pos > 0 {
                        let removed = pos;
                        state.queue.drain(0..pos);
                        state.queue_position = Some(0);
                        // Adjust selection
                        if let Some(sel) = state.queue_state.selected {
                            if sel < pos {
                                state.queue_state.selected = Some(0);
                            } else {
                                state.queue_state.selected = Some(sel - pos);
                            }
                        }
                        state.notify(format!("Cleared {} played songs", removed));
                    } else {
                        state.notify("No history to clear");
                    }
                } else {
                    state.notify("No history to clear");
                }
            }
            _ => {}
        }

        Ok(())
    }
}
