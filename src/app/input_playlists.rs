use crossterm::event::{self, KeyCode};

use crate::error::Error;

use super::*;

impl App {
    /// Handle playlists page keys
    pub(super) async fn handle_playlists_key(&mut self, key: event::KeyEvent) -> Result<(), Error> {
        let ds = self.daemon_state.read().await;
        let mut cs = self.client_state.write().await;
        let mut state = AppState { daemon: &*ds, client: &mut *cs };

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
                            drop(state); drop(cs); drop(ds);

                            let songs = self.load_playlist(&playlist_id).await;
                            let ds = self.daemon_state.read().await;
                            let mut cs = self.client_state.write().await;
                            let mut state = AppState { daemon: &*ds, client: &mut *cs };
                            let count = songs.len();
                            state.client.playlists.songs = songs;
                            state.client.playlists.selected_song =
                                if count > 0 { Some(0) } else { None };
                            state.client.playlists.focus = 1;
                            state.client.notify(format!(
                                "Loaded playlist: {} ({} songs)",
                                playlist_name, count
                            ));
                            return Ok(());
                        }
                    }
                } else {
                    // Play selected song from playlist
                    if let Some(idx) = state.client.playlists.selected_song {
                        if idx < state.client.playlists.songs.len() {
                            let songs = state.client.playlists.songs.clone();
                            drop(state); drop(cs); drop(ds);
                            return self
                                .client
                                .request(DaemonRequest::EnqueueSongs {
                                    songs,
                                    mode: EnqueueMode::Replace {
                                        play_from: Some(idx),
                                    },
                                })
                                .await
                                .map(|_| ())
                                .map_err(Error::from);
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
                            state.client.notify(format!("Added to queue: {}", title));
                            drop(state); drop(cs); drop(ds);
                            let _ = self
                                .client
                                .request(DaemonRequest::EnqueueSongs {
                                    songs: vec![song],
                                    mode: EnqueueMode::Append,
                                })
                                .await;
                        }
                    }
                } else {
                    // Add whole playlist
                    if !state.client.playlists.songs.is_empty() {
                        let count = state.client.playlists.songs.len();
                        let songs = state.client.playlists.songs.clone();
                        state.client.notify(format!("Added {} songs to queue", count));
                        drop(state); drop(cs); drop(ds);
                        let _ = self
                            .client
                            .request(DaemonRequest::EnqueueSongs {
                                songs,
                                mode: EnqueueMode::Append,
                            })
                            .await;
                    }
                }
            }
            KeyCode::Char('i') => {
                // Add next
                let insert_pos = state.daemon.queue_position;
                if state.client.playlists.focus == 1 {
                    if let Some(idx) = state.client.playlists.selected_song {
                        if let Some(song) = state.client.playlists.songs.get(idx).cloned() {
                            let title = song.title.clone();
                            state.client.notify(format!("Playing next: {}", title));
                            drop(state); drop(cs); drop(ds);
                            let mode = match insert_pos {
                                Some(pos) => EnqueueMode::InsertAfter(pos),
                                None => EnqueueMode::Append,
                            };
                            let _ = self
                                .client
                                .request(DaemonRequest::EnqueueSongs {
                                    songs: vec![song],
                                    mode,
                                })
                                .await;
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
                    drop(state); drop(cs); drop(ds);
                    return self
                        .client
                        .request(DaemonRequest::EnqueueSongs {
                            songs,
                            mode: EnqueueMode::Replace { play_from: Some(0) },
                        })
                        .await
                        .map(|_| ())
                        .map_err(Error::from);
                }
            }
            KeyCode::Char('m') if state.client.playlists.focus == 1 => {
                let song_id = state
                    .client
                    .playlists
                    .selected_song
                    .and_then(|idx| state.client.playlists.songs.get(idx).map(|s| s.id.clone()));
                drop(state); drop(cs); drop(ds);
                if let Some(id) = song_id {
                    let _ = self
                        .client
                        .request(DaemonRequest::ToggleStarSong(id))
                        .await;
                }
                return Ok(());
            }
            _ => {}
        }

        Ok(())
    }
}
