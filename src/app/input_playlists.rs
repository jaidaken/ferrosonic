use crossterm::event::{self, KeyCode};

use crate::error::Error;

use super::*;

impl App {
    /// Handle playlists page keys
    pub(super) async fn handle_playlists_key(&mut self, key: event::KeyEvent) -> Result<(), Error> {
        let mut state = self.state.write().await;

        match key.code {
            KeyCode::Tab => {
                state.playlists.focus = (state.playlists.focus + 1) % 2;
            }
            KeyCode::Left => {
                state.playlists.focus = 0;
            }
            KeyCode::Right => {
                if !state.playlists.songs.is_empty() {
                    state.playlists.focus = 1;
                    if state.playlists.selected_song.is_none() {
                        state.playlists.selected_song = Some(0);
                    }
                }
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if state.playlists.focus == 0 {
                    // Playlist list
                    if let Some(sel) = state.playlists.selected_playlist {
                        if sel > 0 {
                            state.playlists.selected_playlist = Some(sel - 1);
                        }
                    } else if !state.playlists.playlists.is_empty() {
                        state.playlists.selected_playlist = Some(0);
                    }
                } else {
                    // Song list
                    if let Some(sel) = state.playlists.selected_song {
                        if sel > 0 {
                            state.playlists.selected_song = Some(sel - 1);
                        }
                    } else if !state.playlists.songs.is_empty() {
                        state.playlists.selected_song = Some(0);
                    }
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if state.playlists.focus == 0 {
                    let max = state.playlists.playlists.len().saturating_sub(1);
                    if let Some(sel) = state.playlists.selected_playlist {
                        if sel < max {
                            state.playlists.selected_playlist = Some(sel + 1);
                        }
                    } else if !state.playlists.playlists.is_empty() {
                        state.playlists.selected_playlist = Some(0);
                    }
                } else {
                    let max = state.playlists.songs.len().saturating_sub(1);
                    if let Some(sel) = state.playlists.selected_song {
                        if sel < max {
                            state.playlists.selected_song = Some(sel + 1);
                        }
                    } else if !state.playlists.songs.is_empty() {
                        state.playlists.selected_song = Some(0);
                    }
                }
            }
            KeyCode::Enter => {
                if state.playlists.focus == 0 {
                    // Load playlist songs
                    if let Some(idx) = state.playlists.selected_playlist {
                        if let Some(playlist) = state.playlists.playlists.get(idx) {
                            let playlist_id = playlist.id.clone();
                            let playlist_name = playlist.name.clone();
                            drop(state);

                            if let Some(ref client) = self.subsonic {
                                match client.get_playlist(&playlist_id).await {
                                    Ok((_playlist, songs)) => {
                                        let mut state = self.state.write().await;
                                        let count = songs.len();
                                        state.playlists.songs = songs;
                                        state.playlists.selected_song =
                                            if count > 0 { Some(0) } else { None };
                                        state.playlists.focus = 1;
                                        state.notify(format!(
                                            "Loaded playlist: {} ({} songs)",
                                            playlist_name, count
                                        ));
                                    }
                                    Err(e) => {
                                        let mut state = self.state.write().await;
                                        state.notify_error(format!(
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
                    if let Some(idx) = state.playlists.selected_song {
                        if idx < state.playlists.songs.len() {
                            let songs = state.playlists.songs.clone();
                            state.queue.clear();
                            state.queue.extend(songs);
                            drop(state);
                            return self.play_queue_position(idx).await;
                        }
                    }
                }
            }
            KeyCode::Char('e') => {
                // Add to queue
                if state.playlists.focus == 1 {
                    if let Some(idx) = state.playlists.selected_song {
                        if let Some(song) = state.playlists.songs.get(idx).cloned() {
                            let title = song.title.clone();
                            state.queue.push(song);
                            state.notify(format!("Added to queue: {}", title));
                        }
                    }
                } else {
                    // Add whole playlist
                    if !state.playlists.songs.is_empty() {
                        let count = state.playlists.songs.len();
                        let songs = state.playlists.songs.clone();
                        state.queue.extend(songs);
                        state.notify(format!("Added {} songs to queue", count));
                    }
                }
            }
            KeyCode::Char('n') => {
                // Add next
                let insert_pos = state.queue_position.map(|p| p + 1).unwrap_or(0);
                if state.playlists.focus == 1 {
                    if let Some(idx) = state.playlists.selected_song {
                        if let Some(song) = state.playlists.songs.get(idx).cloned() {
                            let title = song.title.clone();
                            state.queue.insert(insert_pos, song);
                            state.notify(format!("Playing next: {}", title));
                        }
                    }
                }
            }
            KeyCode::Char('s') => {
                // Shuffle play playlist
                use rand::seq::SliceRandom;
                if !state.playlists.songs.is_empty() {
                    let mut songs = state.playlists.songs.clone();
                    songs.shuffle(&mut rand::thread_rng());
                    state.queue.clear();
                    state.queue.extend(songs);
                    drop(state);
                    return self.play_queue_position(0).await;
                }
            }
            _ => {}
        }

        Ok(())
    }
}
