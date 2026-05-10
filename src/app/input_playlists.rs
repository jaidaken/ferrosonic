use crossterm::event::{self, KeyCode};

use crate::error::Error;

use super::*;

impl App {
    /// Handle playlists page keys
    pub(super) async fn handle_playlists_key(&mut self, key: event::KeyEvent) -> Result<(), Error> {
        let mut state = self.state.write().await;

        match key.code {
            KeyCode::Tab => {
                state.client.playlists.focus = (state.client.playlists.focus + 1) % 2;
            }
            KeyCode::Left => {
                state.client.playlists.focus = 0;
            }
            KeyCode::Right => {
                if !state.client.playlists.songs.is_empty() {
                    state.client.playlists.focus = 1;
                    if state.client.playlists.selected_song.is_none() {
                        state.client.playlists.selected_song = Some(0);
                    }
                }
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if state.client.playlists.focus == 0 {
                    // Playlist list
                    if let Some(sel) = state.client.playlists.selected_playlist {
                        if sel > 0 {
                            state.client.playlists.selected_playlist = Some(sel - 1);
                        }
                    } else if !state.daemon.library.playlists.is_empty() {
                        state.client.playlists.selected_playlist = Some(0);
                    }
                } else {
                    // Song list
                    if let Some(sel) = state.client.playlists.selected_song {
                        if sel > 0 {
                            state.client.playlists.selected_song = Some(sel - 1);
                        }
                    } else if !state.client.playlists.songs.is_empty() {
                        state.client.playlists.selected_song = Some(0);
                    }
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if state.client.playlists.focus == 0 {
                    let max = state.daemon.library.playlists.len().saturating_sub(1);
                    if let Some(sel) = state.client.playlists.selected_playlist {
                        if sel < max {
                            state.client.playlists.selected_playlist = Some(sel + 1);
                        }
                    } else if !state.daemon.library.playlists.is_empty() {
                        state.client.playlists.selected_playlist = Some(0);
                    }
                } else {
                    let max = state.client.playlists.songs.len().saturating_sub(1);
                    if let Some(sel) = state.client.playlists.selected_song {
                        if sel < max {
                            state.client.playlists.selected_song = Some(sel + 1);
                        }
                    } else if !state.client.playlists.songs.is_empty() {
                        state.client.playlists.selected_song = Some(0);
                    }
                }
            }
            KeyCode::Enter => {
                if state.client.playlists.focus == 0 {
                    // Load playlist songs
                    if let Some(idx) = state.client.playlists.selected_playlist {
                        if let Some(playlist) = state.daemon.library.playlists.get(idx) {
                            let playlist_id = playlist.id.clone();
                            let playlist_name = playlist.name.clone();
                            drop(state);

                            if let Some(ref client) = self.subsonic {
                                match client.get_playlist(&playlist_id).await {
                                    Ok((_playlist, songs)) => {
                                        let mut state = self.state.write().await;
                                        let count = songs.len();
                                        state.client.playlists.songs = songs;
                                        state.client.playlists.selected_song =
                                            if count > 0 { Some(0) } else { None };
                                        state.client.playlists.focus = 1;
                                        state.client.notify(format!(
                                                "Loaded playlist: {} ({} songs)",
                                                playlist_name, count
                                        ));
                                    }
                                    Err(e) => {
                                        let mut state = self.state.write().await;
                                        state.client.notify_error(format!(
                                                "Failed to load playlist: {}",
                                                e
                                        ));
                                    }
                                }
                            }
                            return Ok(());
                        }
                    }
                } else {
                    // Play selected song from playlist
                    if let Some(idx) = state.client.playlists.selected_song {
                        if idx < state.client.playlists.songs.len() {
                            let songs = state.client.playlists.songs.clone();
                            state.daemon.queue.clear();
                            state.daemon.queue.extend(songs);
                            drop(state);
                            return self.play_queue_position(idx).await;
                        }
                    }
                }
            }
            KeyCode::Char('e') => {
                // Add to queue
                if state.client.playlists.focus == 1 {
                    if let Some(idx) = state.client.playlists.selected_song {
                        if let Some(song) = state.client.playlists.songs.get(idx).cloned() {
                            let title = song.title.clone();
                            state.daemon.queue.push(song);
                            state.client.notify(format!("Added to queue: {}", title));
                        }
                    }
                } else {
                    // Add whole playlist
                    if !state.client.playlists.songs.is_empty() {
                        let count = state.client.playlists.songs.len();
                        let songs = state.client.playlists.songs.clone();
                        state.daemon.queue.extend(songs);
                        state.client.notify(format!("Added {} songs to queue", count));
                    }
                }
            }
            KeyCode::Char('n') => {
                // Add next
                let insert_pos = state.daemon.queue_position.map(|p| p + 1).unwrap_or(0);
                if state.client.playlists.focus == 1 {
                    if let Some(idx) = state.client.playlists.selected_song {
                        if let Some(song) = state.client.playlists.songs.get(idx).cloned() {
                            let title = song.title.clone();
                            state.daemon.queue.insert(insert_pos, song);
                            state.client.notify(format!("Playing next: {}", title));
                        }
                    }
                }
            }
            KeyCode::Char('r') => {
                // Shuffle play playlist
                use rand::seq::SliceRandom;
                if !state.client.playlists.songs.is_empty() {
                    let mut songs = state.client.playlists.songs.clone();
                    songs.shuffle(&mut rand::thread_rng());
                    state.daemon.queue.clear();
                    state.daemon.queue.extend(songs);
                    drop(state);
                    return self.play_queue_position(0).await;
                }
            }
            _ => {}
        }

        Ok(())
    }
}
