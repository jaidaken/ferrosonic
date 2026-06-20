use crossterm::event::{self, KeyCode};

use crate::error::Error;

use super::*;

impl App {
    pub(super) async fn handle_playlists_key(&mut self, key: event::KeyEvent) -> Result<(), Error> {
        let ds = self.daemon_state.read().await;
        let mut cs = self.client_state.write().await;
        let state = AppState {
            daemon: &ds,
            client: &mut cs,
        };

        // Rename box owns all keys while open.
        if state.client.playlists.renaming {
            match key.code {
                KeyCode::Esc => {
                    state.client.playlists.renaming = false;
                    state.client.playlists.rename_buf.clear();
                }
                KeyCode::Backspace => {
                    state.client.playlists.rename_buf.pop();
                }
                KeyCode::Char(c) => {
                    state.client.playlists.rename_buf.push(c);
                }
                KeyCode::Enter => {
                    let name = state.client.playlists.rename_buf.trim().to_string();
                    let id = state
                        .client
                        .playlists
                        .selected_playlist
                        .and_then(|i| state.daemon.library.playlists.get(i))
                        .map(|p| p.id.clone());
                    state.client.playlists.renaming = false;
                    state.client.playlists.rename_buf.clear();
                    if name.is_empty() {
                        state.client.notify("Playlist name cannot be empty");
                        return Ok(());
                    }
                    let Some(id) = id else { return Ok(()) };
                    state.client.notify(format!("Renamed playlist to: {name}"));
                    drop(state);
                    drop(cs);
                    drop(ds);
                    let _ = self
                        .client
                        .request(DaemonRequest::RenamePlaylist { id, name })
                        .await;
                    return Ok(());
                }
                _ => {}
            }
            return Ok(());
        }

        // Delete-confirmation prompt owns y/n/esc while open.
        if state.client.playlists.confirming_delete {
            match key.code {
                KeyCode::Char('y') | KeyCode::Char('Y') => {
                    let id = state
                        .client
                        .playlists
                        .selected_playlist
                        .and_then(|i| state.daemon.library.playlists.get(i))
                        .map(|p| p.id.clone());
                    state.client.playlists.confirming_delete = false;
                    let Some(id) = id else { return Ok(()) };
                    state.client.playlists.songs.clear();
                    state.client.playlists.selected_song = None;
                    state.client.notify("Deleted playlist");
                    drop(state);
                    drop(cs);
                    drop(ds);
                    let _ = self
                        .client
                        .request(DaemonRequest::DeletePlaylist { id })
                        .await;
                    return Ok(());
                }
                KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                    state.client.playlists.confirming_delete = false;
                }
                _ => {}
            }
            return Ok(());
        }

        match key.code {
            KeyCode::Tab => {
                state.client.playlists.focus = (state.client.playlists.focus + 1) % 2;
            }
            KeyCode::Left => {
                state.client.playlists.focus = 0;
            }
            KeyCode::Right if !state.client.playlists.songs.is_empty() => {
                state.client.playlists.focus = 1;
                if state.client.playlists.selected_song.is_none() {
                    state.client.playlists.selected_song = Some(0);
                }
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if state.client.playlists.focus == 0 {
                    if let Some(sel) = state.client.playlists.selected_playlist {
                        if sel > 0 {
                            state.client.playlists.selected_playlist = Some(sel - 1);
                        }
                    } else if !state.daemon.library.playlists.is_empty() {
                        state.client.playlists.selected_playlist = Some(0);
                    }
                } else if let Some(sel) = state.client.playlists.selected_song {
                    if sel > 0 {
                        state.client.playlists.selected_song = Some(sel - 1);
                    }
                } else if !state.client.playlists.songs.is_empty() {
                    state.client.playlists.selected_song = Some(0);
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
                    if let Some(idx) = state.client.playlists.selected_playlist {
                        if let Some(playlist) = state.daemon.library.playlists.get(idx) {
                            let playlist_id = playlist.id.clone();
                            let playlist_name = playlist.name.clone();
                            drop(state);
                            drop(cs);
                            drop(ds);

                            let songs = self.load_playlist(&playlist_id).await;
                            let ds = self.daemon_state.read().await;
                            let mut cs = self.client_state.write().await;
                            let state = AppState {
                                daemon: &ds,
                                client: &mut cs,
                            };
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
                } else if let Some(idx) = state.client.playlists.selected_song {
                    if idx < state.client.playlists.songs.len() {
                        let songs = state.client.playlists.songs.clone();
                        drop(state);
                        drop(cs);
                        drop(ds);
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
            KeyCode::Char('e') => {
                if state.client.playlists.focus == 1 {
                    if let Some(idx) = state.client.playlists.selected_song {
                        if let Some(song) = state.client.playlists.songs.get(idx).cloned() {
                            let title = song.title.clone();
                            state.client.notify(format!("Added to queue: {}", title));
                            drop(state);
                            drop(cs);
                            drop(ds);
                            let _ = self
                                .client
                                .request(DaemonRequest::EnqueueSongs {
                                    songs: vec![song],
                                    mode: EnqueueMode::Append,
                                })
                                .await;
                        }
                    }
                } else if !state.client.playlists.songs.is_empty() {
                    let count = state.client.playlists.songs.len();
                    let songs = state.client.playlists.songs.clone();
                    state
                        .client
                        .notify(format!("Added {} songs to queue", count));
                    drop(state);
                    drop(cs);
                    drop(ds);
                    let _ = self
                        .client
                        .request(DaemonRequest::EnqueueSongs {
                            songs,
                            mode: EnqueueMode::Append,
                        })
                        .await;
                }
            }
            KeyCode::Char('i') => {
                let insert_pos = state.daemon.queue_position;
                if state.client.playlists.focus == 1 {
                    if let Some(idx) = state.client.playlists.selected_song {
                        if let Some(song) = state.client.playlists.songs.get(idx).cloned() {
                            let title = song.title.clone();
                            state.client.notify(format!("Playing next: {}", title));
                            drop(state);
                            drop(cs);
                            drop(ds);
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
            KeyCode::Char('t') => {
                use rand::seq::SliceRandom;
                if !state.client.playlists.songs.is_empty() {
                    let mut songs = state.client.playlists.songs.clone();
                    songs.shuffle(&mut rand::thread_rng());
                    drop(state);
                    drop(cs);
                    drop(ds);
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
                let song_id =
                    state.client.playlists.selected_song.and_then(|idx| {
                        state.client.playlists.songs.get(idx).map(|s| s.id.clone())
                    });
                drop(state);
                drop(cs);
                drop(ds);
                if let Some(id) = song_id {
                    let _ = self.client.request(DaemonRequest::ToggleStarSong(id)).await;
                }
                return Ok(());
            }
            KeyCode::Char('R') if state.client.playlists.focus == 0 => {
                if let Some(p) = state
                    .client
                    .playlists
                    .selected_playlist
                    .and_then(|i| state.daemon.library.playlists.get(i))
                {
                    state.client.playlists.rename_buf = p.name.clone();
                    state.client.playlists.renaming = true;
                }
            }
            KeyCode::Char('D') if state.client.playlists.focus == 0 => {
                if state
                    .client
                    .playlists
                    .selected_playlist
                    .and_then(|i| state.daemon.library.playlists.get(i))
                    .is_some()
                {
                    state.client.playlists.confirming_delete = true;
                }
            }
            KeyCode::Char('d') if state.client.playlists.focus == 1 => {
                let playlist_id = state
                    .client
                    .playlists
                    .selected_playlist
                    .and_then(|i| state.daemon.library.playlists.get(i))
                    .map(|p| p.id.clone());
                let index = state.client.playlists.selected_song;
                if let (Some(playlist_id), Some(index)) = (playlist_id, index) {
                    if index < state.client.playlists.songs.len() {
                        state.client.playlists.songs.remove(index);
                        let len = state.client.playlists.songs.len();
                        state.client.playlists.selected_song = match len {
                            0 => None,
                            _ => Some(index.min(len - 1)),
                        };
                        state.client.notify("Removed song from playlist");
                        drop(state);
                        drop(cs);
                        drop(ds);
                        if let Ok(crate::ipc::DaemonResponse::PlaylistSongs(songs)) = self
                            .client
                            .request(DaemonRequest::RemovePlaylistSong { playlist_id, index })
                            .await
                        {
                            self.reconcile_playlist_songs(songs).await;
                        }
                        return Ok(());
                    }
                }
            }
            KeyCode::Char('J') if state.client.playlists.focus == 1 => {
                drop(state);
                drop(cs);
                drop(ds);
                return self.move_playlist_song(1).await;
            }
            KeyCode::Char('K') if state.client.playlists.focus == 1 => {
                drop(state);
                drop(cs);
                drop(ds);
                return self.move_playlist_song(-1).await;
            }
            KeyCode::Char('a') if state.client.playlists.focus == 1 => {
                let song = state
                    .client
                    .playlists
                    .selected_song
                    .and_then(|i| state.client.playlists.songs.get(i))
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

    /// Key handler for the add-to-playlist picker overlay; owns all input
    /// while open. Enter adds the held song to the highlighted playlist.
    pub(super) async fn handle_playlist_picker_key(
        &mut self,
        key: event::KeyEvent,
    ) -> Result<(), Error> {
        let ds = self.daemon_state.read().await;
        let mut cs = self.client_state.write().await;
        let count = ds.library.playlists.len();
        match key.code {
            KeyCode::Esc => {
                cs.playlist_picker.active = false;
                cs.playlist_picker.song = None;
            }
            KeyCode::Up | KeyCode::Char('k') => {
                cs.playlist_picker.selected = cs.playlist_picker.selected.saturating_sub(1);
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if cs.playlist_picker.selected + 1 < count {
                    cs.playlist_picker.selected += 1;
                }
            }
            KeyCode::Enter => {
                let target = ds
                    .library
                    .playlists
                    .get(cs.playlist_picker.selected)
                    .map(|p| (p.id.clone(), p.name.clone()));
                let song = cs.playlist_picker.song.clone();
                cs.playlist_picker.active = false;
                cs.playlist_picker.song = None;
                if let (Some((playlist_id, pname)), Some(song)) = (target, song) {
                    cs.notify(format!("Added '{}' to {}", song.title, pname));
                    drop(cs);
                    drop(ds);
                    let _ = self
                        .client
                        .request(DaemonRequest::AddSongToPlaylist {
                            playlist_id,
                            song_id: song.id,
                        })
                        .await;
                    return Ok(());
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// Move the highlighted playlist song by `delta` (+1 down, -1 up) and
    /// persist the new order. Optimistic: the local pane already mirrors the
    /// server list, so the reordered ids are authoritative.
    async fn move_playlist_song(&mut self, delta: isize) -> Result<(), Error> {
        let ds = self.daemon_state.read().await;
        let mut cs = self.client_state.write().await;
        let len = cs.playlists.songs.len();
        let Some(i) = cs.playlists.selected_song else {
            return Ok(());
        };
        let j = i as isize + delta;
        if j < 0 || j as usize >= len {
            return Ok(());
        }
        let j = j as usize;
        let playlist_id = cs
            .playlists
            .selected_playlist
            .and_then(|p| ds.library.playlists.get(p))
            .map(|p| p.id.clone());
        let Some(playlist_id) = playlist_id else {
            return Ok(());
        };
        cs.playlists.songs.swap(i, j);
        cs.playlists.selected_song = Some(j);
        let song_ids: Vec<String> = cs.playlists.songs.iter().map(|s| s.id.clone()).collect();
        drop(cs);
        drop(ds);
        if let Ok(crate::ipc::DaemonResponse::PlaylistSongs(songs)) = self
            .client
            .request(DaemonRequest::ReorderPlaylist {
                playlist_id,
                song_ids,
            })
            .await
        {
            self.reconcile_playlist_songs(songs).await;
        }
        Ok(())
    }

    /// Replace the songs pane with the daemon's authoritative list after an
    /// edit, clamping the selection so an optimistic update cannot diverge.
    async fn reconcile_playlist_songs(&mut self, songs: Vec<crate::subsonic::models::Child>) {
        let mut cs = self.client_state.write().await;
        let len = songs.len();
        cs.playlists.songs = songs;
        cs.playlists.selected_song = match len {
            0 => None,
            _ => Some(cs.playlists.selected_song.unwrap_or(0).min(len - 1)),
        };
    }
}
