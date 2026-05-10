use crossterm::event::{self, KeyCode};
use tracing::info;

use crate::error::Error;

use super::*;

use rand::seq::SliceRandom;
use rand::thread_rng;

impl App {
    /// Handle artists page keys
    pub(super) async fn handle_artists_key(&mut self, key: event::KeyEvent) -> Result<(), Error> {
        use crate::ui::pages::artists::{build_tree_items, TreeItem};

        let mut state = self.state.write().await;

        // Handle filter input mode
        if state.client.artists.filter_active {
            match key.code {
                KeyCode::Esc => {
                    state.client.artists.filter_active = false;
                    state.client.artists.filter.clear();
                }
                KeyCode::Enter => {
                    state.client.artists.filter_active = false;
                }
                KeyCode::Backspace => {
                    state.client.artists.filter.pop();
                }
                KeyCode::Char(c) => {
                    state.client.artists.filter.push(c);
                }
                _ => {}
            }
            return Ok(());
        }

        match key.code {
            KeyCode::Char('/') => {
                state.client.artists.filter_active = true;
            }
            KeyCode::Esc => {
                state.client.artists.filter.clear();
                state.client.artists.expanded.clear();
                state.client.artists.selected_index = Some(0);
            }
            KeyCode::Tab => {
                state.client.artists.focus = (state.client.artists.focus + 1) % 2;
            }
            KeyCode::Left => {
                state.client.artists.focus = 0;
            }
            KeyCode::Right => {
                // Move focus to songs (right pane)
                if !state.client.artists.songs.is_empty() {
                    state.client.artists.focus = 1;
                    if state.client.artists.selected_song.is_none() {
                        state.client.artists.selected_song = Some(0);
                    }
                }
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if state.client.artists.focus == 0 {
                    // Tree navigation
                    let tree_items = build_tree_items(&state);
                    if let Some(sel) = state.client.artists.selected_index {
                        if sel > 0 {
                            state.client.artists.selected_index = Some(sel - 1);
                        }
                    } else if !tree_items.is_empty() {
                        state.client.artists.selected_index = Some(0);
                    }
                    // Preview album songs in right pane
                    let album_id = state.client
                        .artists
                        .selected_index
                        .and_then(|i| tree_items.get(i))
                        .and_then(|item| match item {
                            TreeItem::Album { album } => Some(album.id.clone()),
                            _ => None,
                        });
                    if let Some(album_id) = album_id {
                        drop(state);
                        let songs = self.load_album(&album_id).await;
                        if !songs.is_empty() {
                            let mut state = self.state.write().await;
                            state.client.artists.songs = songs;
                            state.client.artists.selected_song = Some(0);
                        }
                        return Ok(());
                    }
                } else {
                    // Song list
                    if let Some(sel) = state.client.artists.selected_song {
                        if sel > 0 {
                            state.client.artists.selected_song = Some(sel - 1);
                        }
                    } else if !state.client.artists.songs.is_empty() {
                        state.client.artists.selected_song = Some(0);
                    }
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if state.client.artists.focus == 0 {
                    // Tree navigation
                    let tree_items = build_tree_items(&state);
                    let max = tree_items.len().saturating_sub(1);
                    if let Some(sel) = state.client.artists.selected_index {
                        if sel < max {
                            state.client.artists.selected_index = Some(sel + 1);
                        }
                    } else if !tree_items.is_empty() {
                        state.client.artists.selected_index = Some(0);
                    }
                    // Preview album songs in right pane
                    let album_id = state.client
                        .artists
                        .selected_index
                        .and_then(|i| tree_items.get(i))
                        .and_then(|item| match item {
                            TreeItem::Album { album } => Some(album.id.clone()),
                            _ => None,
                        });
                    if let Some(album_id) = album_id {
                        drop(state);
                        let songs = self.load_album(&album_id).await;
                        if !songs.is_empty() {
                            let mut state = self.state.write().await;
                            state.client.artists.songs = songs;
                            state.client.artists.selected_song = Some(0);
                        }
                        return Ok(());
                    }
                } else {
                    // Song list
                    let max = state.client.artists.songs.len().saturating_sub(1);
                    if let Some(sel) = state.client.artists.selected_song {
                        if sel < max {
                            state.client.artists.selected_song = Some(sel + 1);
                        }
                    } else if !state.client.artists.songs.is_empty() {
                        state.client.artists.selected_song = Some(0);
                    }
                }
            }
            KeyCode::Char('s') => {
                if state.client.artists.focus == 0 {
                    let tree_items = build_tree_items(&state);
                    if let Some(idx) = state.client.artists.selected_index {
                        if let Some(item) = tree_items.get(idx) {
                            match item {
                                TreeItem::Artist {
                                    artist,
                                    expanded: _,
                                } => {
                                    let artist_id = artist.id.clone();
                                    let artist_name = artist.name.clone();

                                    drop(state);

                                    let albums_resp = self
                                        .client
                                        .request(DaemonRequest::LoadArtist(artist_id.clone()))
                                        .await;
                                    let albums = match albums_resp {
                                        Ok(crate::ipc::DaemonResponse::ArtistAlbums(a)) => a,
                                        _ => Vec::new(),
                                    };
                                    if !albums.is_empty() {
                                        let mut artists_songs: Vec<_> = Vec::new();
                                        for album in albums {
                                            let songs = self.load_album(&album.id).await;
                                            artists_songs.extend(songs);
                                        }

                                        if artists_songs.is_empty() {
                                            let mut state = self.state.write().await;
                                            state.client.notify_error(format!(
                                                "No songs found for {}",
                                                artist_name,
                                            ));
                                            return Ok(());
                                        }

                                        artists_songs.shuffle(&mut thread_rng());

                                        let song_count = artists_songs.len();
                                        {
                                            let mut state = self.state.write().await;
                                            state.client.notify(format!(
                                                "Shuffling {} songs by {}",
                                                song_count, artist_name
                                            ));
                                        }

                                        return self
                                            .client
                                            .request(DaemonRequest::EnqueueSongs {
                                                songs: artists_songs,
                                                mode: EnqueueMode::Replace {
                                                    play_from: Some(0),
                                                },
                                            })
                                            .await
                                            .map(|_| ())
                                            .map_err(Error::from);
                                    }
                                }
                                TreeItem::Album { album } => {
                                    let album_id = album.id.clone();
                                    let album_name = album.name.clone();

                                    drop(state);

                                    let songs = self.load_album(&album_id).await;
                                    if songs.is_empty() {
                                        let mut state = self.state.write().await;
                                        state.client.notify_error("Album has no songs");
                                        return Ok(());
                                    }

                                    let mut shuffled_songs = songs;
                                    shuffled_songs.shuffle(&mut thread_rng());

                                    {
                                        let mut state = self.state.write().await;
                                        state.client.notify(format!("Shuffling {}", album_name));
                                    }

                                    return self
                                        .client
                                        .request(DaemonRequest::EnqueueSongs {
                                            songs: shuffled_songs,
                                            mode: EnqueueMode::Replace {
                                                play_from: Some(0),
                                            },
                                        })
                                        .await
                                        .map(|_| ())
                                        .map_err(Error::from);
                                }
                            }
                        }
                    }
                }
            }
            KeyCode::Enter => {
                if state.client.artists.focus == 0 {
                    // Get current tree item
                    let tree_items = build_tree_items(&state);
                    if let Some(idx) = state.client.artists.selected_index {
                        if let Some(item) = tree_items.get(idx) {
                            match item {
                                TreeItem::Artist { artist, expanded } => {
                                    let artist_id = artist.id.clone();
                                    let artist_name = artist.name.clone();
                                    let was_expanded = *expanded;

                                    if was_expanded {
                                        state.client.artists.expanded.remove(&artist_id);
                                    } else if !state.daemon.library.albums_cache.contains_key(&artist_id) {
                                        drop(state);
                                        match self
                                            .client
                                            .request(DaemonRequest::LoadArtist(artist_id.clone()))
                                            .await
                                        {
                                            Ok(crate::ipc::DaemonResponse::ArtistAlbums(_)) => {
                                                // Daemon already cached + emitted
                                                // AlbumsChanged; the event-pump task
                                                // mirrors that into our local cache.
                                                let mut state = self.state.write().await;
                                                state.client.artists.expanded.insert(artist_id);
                                                info!("Loaded albums for {}", artist_name);
                                            }
                                            _ => {
                                                let mut state = self.state.write().await;
                                                state.client.notify_error("Failed to load artist");
                                            }
                                        }
                                        return Ok(());
                                    } else {
                                        state.client.artists.expanded.insert(artist_id);
                                    }
                                }
                                TreeItem::Album { album } => {
                                    let album_id = album.id.clone();
                                    let album_name = album.name.clone();
                                    drop(state);

                                    let songs = self.load_album(&album_id).await;
                                    if songs.is_empty() {
                                        let mut state = self.state.write().await;
                                        state.client.notify_error("Album has no songs");
                                        return Ok(());
                                    }

                                    {
                                        let mut state = self.state.write().await;
                                        let count = songs.len();
                                        state.client.artists.songs = songs.clone();
                                        state.client.artists.selected_song = Some(0);
                                        state.client.artists.focus = 1;
                                        state.client.notify(format!(
                                            "Playing album: {} ({} songs)",
                                            album_name, count
                                        ));
                                    }
                                    let _ = self
                                        .client
                                        .request(DaemonRequest::EnqueueSongs {
                                            songs,
                                            mode: EnqueueMode::Replace {
                                                play_from: Some(0),
                                            },
                                        })
                                        .await;
                                    return Ok(());
                                }
                            }
                        }
                    }
                } else {
                    // Play selected song from current position
                    if let Some(idx) = state.client.artists.selected_song {
                        if idx < state.client.artists.songs.len() {
                            let songs = state.client.artists.songs.clone();
                            if let Some(song) = songs.get(idx) {
                                state.client.notify(format!("Playing: {}", song.title));
                            }
                            drop(state);
                            let _ = self
                                .client
                                .request(DaemonRequest::EnqueueSongs {
                                    songs,
                                    mode: EnqueueMode::Replace {
                                        play_from: Some(idx),
                                    },
                                })
                                .await;
                            return Ok(());
                        }
                    }
                }
            }
            KeyCode::Backspace => {
                if state.client.artists.focus == 1 {
                    state.client.artists.focus = 0;
                }
            }
            KeyCode::Char('e') => {
                if state.client.artists.focus == 1 {
                    if let Some(idx) = state.client.artists.selected_song {
                        if let Some(song) = state.client.artists.songs.get(idx).cloned() {
                            let title = song.title.clone();
                            state.client.notify(format!("Added to queue: {}", title));
                            drop(state);
                            let _ = self
                                .client
                                .request(DaemonRequest::EnqueueSongs {
                                    songs: vec![song],
                                    mode: EnqueueMode::Append,
                                })
                                .await;
                        }
                    }
                } else if !state.client.artists.songs.is_empty() {
                    let count = state.client.artists.songs.len();
                    let songs = state.client.artists.songs.clone();
                    state.client.notify(format!("Added {} songs to queue", count));
                    drop(state);
                    let _ = self
                        .client
                        .request(DaemonRequest::EnqueueSongs {
                            songs,
                            mode: EnqueueMode::Append,
                        })
                        .await;
                }
            }
            KeyCode::Char('n') => {
                let cur_pos = state.daemon.queue_position;
                if state.client.artists.focus == 1 {
                    if let Some(idx) = state.client.artists.selected_song {
                        if let Some(song) = state.client.artists.songs.get(idx).cloned() {
                            let title = song.title.clone();
                            state.client.notify(format!("Playing next: {}", title));
                            drop(state);
                            let mode = match cur_pos {
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
                } else if !state.client.artists.songs.is_empty() {
                    let count = state.client.artists.songs.len();
                    let songs = state.client.artists.songs.clone();
                    state.client.notify(format!("Playing {} songs next", count));
                    drop(state);
                    let mode = match cur_pos {
                        Some(pos) => EnqueueMode::InsertAfter(pos),
                        None => EnqueueMode::Append,
                    };
                    let _ = self
                        .client
                        .request(DaemonRequest::EnqueueSongs { songs, mode })
                        .await;
                }
            }
            _ => {}
        }

        Ok(())
    }
}
