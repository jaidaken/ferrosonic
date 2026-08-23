use crossterm::event::{self, KeyCode};

use crate::error::Error;

use super::{App, AppState, DaemonRequest};

impl App {
    // Cohesive single match/render; splitting would fragment one logical unit.
    #[allow(clippy::too_many_lines)]
    // significant_drop_tightening: tokio guard held to scope; not tightened (early-drop is borrow-blocked, spans a trailing await, or saves nothing before return).
    #[allow(clippy::significant_drop_tightening)]
    pub(super) async fn handle_queue_key(&self, key: event::KeyEvent) -> Result<(), Error> {
        let ds = self.daemon_state.read().await;
        let mut cs = self.client_state.write().await;
        let state = AppState {
            daemon: &ds,
            client: &mut cs,
        };

        // Save-as-playlist name box owns all keys while open.
        if state.client.queue_state.naming_playlist {
            match key.code {
                KeyCode::Esc => {
                    state.client.queue_state.naming_playlist = false;
                    state.client.queue_state.playlist_name.clear();
                }
                KeyCode::Backspace => {
                    state.client.queue_state.playlist_name.pop();
                }
                KeyCode::Char(c) => {
                    state.client.queue_state.playlist_name.push(c);
                }
                KeyCode::Enter => {
                    let name = state.client.queue_state.playlist_name.trim().to_string();
                    if name.is_empty() {
                        state.client.notify("Playlist name cannot be empty");
                        return Ok(());
                    }
                    // Stations are not library songs; createPlaylist rejects a radio: id.
                    let song_ids: Vec<String> = state
                        .daemon
                        .queue
                        .iter()
                        .filter(|s| !s.is_radio())
                        .map(|s| s.id.clone())
                        .collect();
                    state.client.queue_state.naming_playlist = false;
                    state.client.queue_state.playlist_name.clear();
                    if song_ids.is_empty() {
                        let msg = if state.daemon.queue.is_empty() {
                            "Queue is empty"
                        } else {
                            "Queue holds only radio stations"
                        };
                        state.client.notify(msg);
                        return Ok(());
                    }
                    let count = song_ids.len();
                    let _ = state;
                    drop(cs);
                    drop(ds);
                    let saved = self
                        .client
                        .request(DaemonRequest::CreatePlaylist {
                            name: name.clone(),
                            song_ids,
                        })
                        .await
                        .is_ok();
                    let mut cs = self.client_state.write().await;
                    if saved {
                        cs.notify(format!("Saved playlist: {name} ({count} songs)"));
                    } else {
                        cs.notify_error(format!("Failed to save playlist: {name}"));
                    }
                    return Ok(());
                }
                _ => {}
            }
            return Ok(());
        }

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
                        let _ = state;
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
                        state.client.notify(format!("Removed: {removed_title}"));
                        let _ = state;
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
                        let _ = state;
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
                        let _ = state;
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
                let _ = state;
                drop(cs);
                drop(ds);
                let _ = self.client.request(DaemonRequest::ShuffleQueue).await;
                return Ok(());
            }
            KeyCode::Char('s') => {
                if state.daemon.queue.is_empty() {
                    state.client.notify("Queue is empty");
                } else {
                    state.client.queue_state.naming_playlist = true;
                    state.client.queue_state.playlist_name.clear();
                }
            }
            KeyCode::Char('c') => {
                let pos = state.daemon.queue_position;
                let sel_before = state.client.queue_state.selected;
                let _ = state;
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
                            .notify(format!("Cleared {removed} played songs"));
                        // Re-anchor client selection to the same song post-trim.
                        if let (Some(p), Some(sel)) = (pos, sel_before) {
                            state.client.queue_state.selected = Some(sel.saturating_sub(p));
                        }
                    }
                }
                return Ok(());
            }
            KeyCode::Char('m') => {
                // A station has no server-side star; the request would be rejected and the row would flicker.
                let song_id = state
                    .client
                    .queue_state
                    .selected
                    .and_then(|idx| state.daemon.queue.get(idx))
                    .filter(|s| !s.is_radio())
                    .map(|s| s.id.clone());
                let _ = state;
                drop(cs);
                drop(ds);
                if let Some(id) = song_id {
                    let _ = self.client.request(DaemonRequest::ToggleStarSong(id)).await;
                }
                return Ok(());
            }
            KeyCode::Char('a') => {
                let idx = state.client.queue_state.selected;
                // A station is not a library song; updatePlaylist would reject the radio: id.
                let song = idx
                    .and_then(|i| state.daemon.queue.get(i))
                    .filter(|s| !s.is_radio())
                    .cloned();
                if let Some(song) = song {
                    if state.daemon.library.playlists.is_empty() {
                        state.client.notify("No playlists to add to");
                    } else {
                        state.client.open_playlist_picker(song);
                    }
                }
            }
            _ => {}
        }

        Ok(())
    }
}
