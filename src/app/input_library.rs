use crossterm::event::{self, KeyCode};
use tracing::info;

use crate::error::Error;

use super::*;

use rand::seq::SliceRandom;
use rand::thread_rng;

impl App {
    pub(super) async fn handle_library_key(&mut self, key: event::KeyEvent) -> Result<(), Error> {
        use crate::ui::pages::library::{build_tree_items, TreeItem};

        let ds = self.daemon_state.read().await;

        let mut cs = self.client_state.write().await;

        let state = AppState {
            daemon: &ds,
            client: &mut cs,
        };

        if state.client.artists.filter_active {
            let mut scope_or_query_changed = false;
            match key.code {
                KeyCode::Esc => {
                    state.client.artists.filter_active = false;
                    state.client.artists.filter.clear();
                    state.client.artists.search_results = None;
                    state.client.artists.filter_scope = Default::default();
                    drop(state);
                    drop(cs);
                    drop(ds);
                    return Ok(());
                }
                KeyCode::Enter => {
                    state.client.artists.filter_active = false;
                    drop(state);
                    drop(cs);
                    drop(ds);
                    return Ok(());
                }
                KeyCode::Backspace => {
                    state.client.artists.filter.pop();
                    scope_or_query_changed = true;
                }
                KeyCode::Char('/') => {
                    // Empty filter: cycle scope. Non-empty: append literal '/'.
                    if state.client.artists.filter.is_empty() {
                        let new_scope = state.client.artists.filter_scope.cycle();
                        state.client.artists.filter_scope = new_scope;
                        state.client.artists.search_results = None;
                        let label = new_scope.label();
                        state.client.notify(format!("Filter: {}", label));
                    } else {
                        state.client.artists.filter.push('/');
                        scope_or_query_changed = true;
                    }
                }
                KeyCode::Char(c) => {
                    state.client.artists.filter.push(c);
                    scope_or_query_changed = true;
                }
                _ => {}
            }
            if !scope_or_query_changed {
                drop(state);
                drop(cs);
                drop(ds);
                return Ok(());
            }
            state.client.artists.search_gen = state.client.artists.search_gen.wrapping_add(1);
            let gen = state.client.artists.search_gen;
            let query = state.client.artists.filter.clone();
            drop(state);
            drop(cs);
            drop(ds);
            if query.is_empty() {
                let mut cs = self.client_state.write().await;
                cs.artists.search_results = None;
                return Ok(());
            }
            let client = self.client.clone();
            let client_state = self.client_state.clone();
            tokio::spawn(async move {
                let resp = client
                    .request(DaemonRequest::Search {
                        query,
                        artist_count: 100,
                        album_count: 100,
                        song_count: 200,
                    })
                    .await;
                if let Ok(crate::ipc::DaemonResponse::SearchResults(r)) = resp {
                    let mut cs = client_state.write().await;
                    // Stale: user typed again since this request was issued.
                    if cs.artists.search_gen == gen {
                        cs.artists.search_results = Some(r);
                    }
                }
            });
            return Ok(());
        }

        match key.code {
            KeyCode::Char('/') => {
                state.client.artists.filter_active = true;
            }
            KeyCode::Esc => {
                state.client.artists.filter.clear();
                state.client.artists.search_results = None;
                state.client.artists.filter_scope = Default::default();
                state.client.artists.expanded.clear();
                state.client.artists.selected_index = Some(0);
            }
            KeyCode::Tab => {
                state.client.artists.focus = (state.client.artists.focus + 1) % 2;
            }
            KeyCode::Left => {
                state.client.artists.focus = 0;
            }
            KeyCode::Right if !state.client.artists.songs.is_empty() => {
                state.client.artists.focus = 1;
                if state.client.artists.selected_song.is_none() {
                    state.client.artists.selected_song = Some(0);
                }
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if state.client.artists.focus == 0 {
                    let tree_items = build_tree_items(&state);
                    if let Some(sel) = state.client.artists.selected_index {
                        if sel > 0 {
                            state.client.artists.selected_index = Some(sel - 1);
                        }
                    } else if !tree_items.is_empty() {
                        state.client.artists.selected_index = Some(0);
                    }
                    let album_id = state
                        .client
                        .artists
                        .selected_index
                        .and_then(|i| tree_items.get(i))
                        .and_then(|item| match item {
                            TreeItem::Album { album } => Some(album.id.clone()),
                            _ => None,
                        });
                    if let Some(album_id) = album_id {
                        drop(state);
                        drop(cs);
                        drop(ds);
                        let songs = self.load_album(&album_id).await;
                        if !songs.is_empty() {
                            let ds = self.daemon_state.read().await;
                            let mut cs = self.client_state.write().await;
                            let state = AppState {
                                daemon: &ds,
                                client: &mut cs,
                            };
                            state.client.artists.songs = songs;
                            state.client.artists.selected_song = Some(0);
                        }
                        return Ok(());
                    }
                } else if let Some(sel) = state.client.artists.selected_song {
                    if sel > 0 {
                        state.client.artists.selected_song = Some(sel - 1);
                    }
                } else if !state.client.artists.songs.is_empty() {
                    state.client.artists.selected_song = Some(0);
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if state.client.artists.focus == 0 {
                    let tree_items = build_tree_items(&state);
                    let max = tree_items.len().saturating_sub(1);
                    if let Some(sel) = state.client.artists.selected_index {
                        if sel < max {
                            state.client.artists.selected_index = Some(sel + 1);
                        }
                    } else if !tree_items.is_empty() {
                        state.client.artists.selected_index = Some(0);
                    }
                    let album_id = state
                        .client
                        .artists
                        .selected_index
                        .and_then(|i| tree_items.get(i))
                        .and_then(|item| match item {
                            TreeItem::Album { album } => Some(album.id.clone()),
                            _ => None,
                        });
                    if let Some(album_id) = album_id {
                        drop(state);
                        drop(cs);
                        drop(ds);
                        let songs = self.load_album(&album_id).await;
                        if !songs.is_empty() {
                            let ds = self.daemon_state.read().await;
                            let mut cs = self.client_state.write().await;
                            let state = AppState {
                                daemon: &ds,
                                client: &mut cs,
                            };
                            state.client.artists.songs = songs;
                            state.client.artists.selected_song = Some(0);
                        }
                        return Ok(());
                    }
                } else {
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
            KeyCode::Char('t') if state.client.artists.focus == 0 => {
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
                                drop(cs);
                                drop(ds);

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
                                        let ds = self.daemon_state.read().await;
                                        let mut cs = self.client_state.write().await;
                                        let state = AppState {
                                            daemon: &ds,
                                            client: &mut cs,
                                        };
                                        state.client.notify_error(format!(
                                            "No songs found for {}",
                                            artist_name,
                                        ));
                                        return Ok(());
                                    }

                                    artists_songs.shuffle(&mut thread_rng());

                                    let song_count = artists_songs.len();
                                    {
                                        let ds = self.daemon_state.read().await;
                                        let mut cs = self.client_state.write().await;
                                        let state = AppState {
                                            daemon: &ds,
                                            client: &mut cs,
                                        };
                                        state.client.notify(format!(
                                            "Shuffling {} songs by {}",
                                            song_count, artist_name
                                        ));
                                    }

                                    return self
                                        .client
                                        .request(DaemonRequest::EnqueueSongs {
                                            songs: artists_songs,
                                            mode: EnqueueMode::Replace { play_from: Some(0) },
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
                                drop(cs);
                                drop(ds);

                                let songs = self.load_album(&album_id).await;
                                if songs.is_empty() {
                                    let ds = self.daemon_state.read().await;
                                    let mut cs = self.client_state.write().await;
                                    let state = AppState {
                                        daemon: &ds,
                                        client: &mut cs,
                                    };
                                    state.client.notify_error("Album has no songs");
                                    return Ok(());
                                }

                                let mut shuffled_songs = songs;
                                shuffled_songs.shuffle(&mut thread_rng());

                                {
                                    let ds = self.daemon_state.read().await;
                                    let mut cs = self.client_state.write().await;
                                    let state = AppState {
                                        daemon: &ds,
                                        client: &mut cs,
                                    };
                                    state.client.notify(format!("Shuffling {}", album_name));
                                }

                                return self
                                    .client
                                    .request(DaemonRequest::EnqueueSongs {
                                        songs: shuffled_songs,
                                        mode: EnqueueMode::Replace { play_from: Some(0) },
                                    })
                                    .await
                                    .map(|_| ())
                                    .map_err(Error::from);
                            }
                            TreeItem::Song { song } => {
                                let song = song.clone();
                                let title = song.title.clone();
                                drop(state);
                                drop(cs);
                                drop(ds);
                                {
                                    let ds = self.daemon_state.read().await;
                                    let mut cs = self.client_state.write().await;
                                    let state = AppState {
                                        daemon: &ds,
                                        client: &mut cs,
                                    };
                                    state.client.notify(format!("Playing: {}", title));
                                }
                                return self
                                    .client
                                    .request(DaemonRequest::EnqueueSongs {
                                        songs: vec![song],
                                        mode: EnqueueMode::Replace { play_from: Some(0) },
                                    })
                                    .await
                                    .map(|_| ())
                                    .map_err(Error::from);
                            }
                        }
                    }
                }
            }
            KeyCode::Enter => {
                if state.client.artists.focus == 0 {
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
                                    } else if !state
                                        .daemon
                                        .library
                                        .albums_cache
                                        .contains_key(&artist_id)
                                    {
                                        drop(state);
                                        drop(cs);
                                        drop(ds);
                                        match self
                                            .client
                                            .request(DaemonRequest::LoadArtist(artist_id.clone()))
                                            .await
                                        {
                                            Ok(crate::ipc::DaemonResponse::ArtistAlbums(_)) => {
                                                // Cache + AlbumsChanged event already emitted by daemon.
                                                let ds = self.daemon_state.read().await;
                                                let mut cs = self.client_state.write().await;
                                                let state = AppState {
                                                    daemon: &ds,
                                                    client: &mut cs,
                                                };
                                                state.client.artists.expanded.insert(artist_id);
                                                info!("Loaded albums for {}", artist_name);
                                            }
                                            _ => {
                                                let ds = self.daemon_state.read().await;
                                                let mut cs = self.client_state.write().await;
                                                let state = AppState {
                                                    daemon: &ds,
                                                    client: &mut cs,
                                                };
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
                                    drop(cs);
                                    drop(ds);

                                    let songs = self.load_album(&album_id).await;
                                    if songs.is_empty() {
                                        let ds = self.daemon_state.read().await;
                                        let mut cs = self.client_state.write().await;
                                        let state = AppState {
                                            daemon: &ds,
                                            client: &mut cs,
                                        };
                                        state.client.notify_error("Album has no songs");
                                        return Ok(());
                                    }

                                    {
                                        let ds = self.daemon_state.read().await;
                                        let mut cs = self.client_state.write().await;
                                        let state = AppState {
                                            daemon: &ds,
                                            client: &mut cs,
                                        };
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
                                            mode: EnqueueMode::Replace { play_from: Some(0) },
                                        })
                                        .await;
                                    return Ok(());
                                }
                                TreeItem::Song { song } => {
                                    let song = song.clone();
                                    let title = song.title.clone();
                                    drop(state);
                                    drop(cs);
                                    drop(ds);
                                    {
                                        let ds = self.daemon_state.read().await;
                                        let mut cs = self.client_state.write().await;
                                        let state = AppState {
                                            daemon: &ds,
                                            client: &mut cs,
                                        };
                                        state.client.notify(format!("Playing: {}", title));
                                    }
                                    let _ = self
                                        .client
                                        .request(DaemonRequest::EnqueueSongs {
                                            songs: vec![song],
                                            mode: EnqueueMode::Replace { play_from: Some(0) },
                                        })
                                        .await;
                                    return Ok(());
                                }
                            }
                        }
                    }
                } else if let Some(idx) = state.client.artists.selected_song {
                    if idx < state.client.artists.songs.len() {
                        let songs = state.client.artists.songs.clone();
                        if let Some(song) = songs.get(idx) {
                            state.client.notify(format!("Playing: {}", song.title));
                        }
                        drop(state);
                        drop(cs);
                        drop(ds);
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
            KeyCode::Backspace if state.client.artists.focus == 1 => {
                state.client.artists.focus = 0;
            }
            KeyCode::Char('e') => {
                if state.client.artists.focus == 1 {
                    if let Some(idx) = state.client.artists.selected_song {
                        if let Some(song) = state.client.artists.songs.get(idx).cloned() {
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
                } else if state.client.artists.focus == 0
                    && !state.client.artists.filter.is_empty()
                    && state.client.artists.search_results.is_some()
                {
                    let tree_items = build_tree_items(&state);
                    if let Some(idx) = state.client.artists.selected_index {
                        if let Some(item) = tree_items.get(idx).cloned() {
                            drop(state);
                            drop(cs);
                            drop(ds);
                            let songs = self.collect_songs_for(&item).await;
                            if !songs.is_empty() {
                                let count = songs.len();
                                {
                                    let ds = self.daemon_state.read().await;
                                    let mut cs = self.client_state.write().await;
                                    let state = AppState {
                                        daemon: &ds,
                                        client: &mut cs,
                                    };
                                    state
                                        .client
                                        .notify(format!("Added {} songs to queue", count));
                                }
                                let _ = self
                                    .client
                                    .request(DaemonRequest::EnqueueSongs {
                                        songs,
                                        mode: EnqueueMode::Append,
                                    })
                                    .await;
                            }
                            return Ok(());
                        }
                    }
                } else if !state.client.artists.songs.is_empty() {
                    let count = state.client.artists.songs.len();
                    let songs = state.client.artists.songs.clone();
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
                let cur_pos = state.daemon.queue_position;
                if state.client.artists.focus == 1 {
                    if let Some(idx) = state.client.artists.selected_song {
                        if let Some(song) = state.client.artists.songs.get(idx).cloned() {
                            let title = song.title.clone();
                            state.client.notify(format!("Playing next: {}", title));
                            drop(state);
                            drop(cs);
                            drop(ds);
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
                } else if state.client.artists.focus == 0
                    && !state.client.artists.filter.is_empty()
                    && state.client.artists.search_results.is_some()
                {
                    let tree_items = build_tree_items(&state);
                    if let Some(idx) = state.client.artists.selected_index {
                        if let Some(item) = tree_items.get(idx).cloned() {
                            drop(state);
                            drop(cs);
                            drop(ds);
                            let songs = self.collect_songs_for(&item).await;
                            if !songs.is_empty() {
                                let count = songs.len();
                                {
                                    let ds = self.daemon_state.read().await;
                                    let mut cs = self.client_state.write().await;
                                    let state = AppState {
                                        daemon: &ds,
                                        client: &mut cs,
                                    };
                                    state.client.notify(format!("Playing {} songs next", count));
                                }
                                let mode = match cur_pos {
                                    Some(pos) => EnqueueMode::InsertAfter(pos),
                                    None => EnqueueMode::Append,
                                };
                                let _ = self
                                    .client
                                    .request(DaemonRequest::EnqueueSongs { songs, mode })
                                    .await;
                            }
                            return Ok(());
                        }
                    }
                } else if !state.client.artists.songs.is_empty() {
                    let count = state.client.artists.songs.len();
                    let songs = state.client.artists.songs.clone();
                    state.client.notify(format!("Playing {} songs next", count));
                    drop(state);
                    drop(cs);
                    drop(ds);
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
            KeyCode::Char('m') if state.client.artists.focus == 1 => {
                let song_id = state
                    .client
                    .artists
                    .selected_song
                    .and_then(|idx| state.client.artists.songs.get(idx).map(|s| s.id.clone()));
                drop(state);
                drop(cs);
                drop(ds);
                if let Some(id) = song_id {
                    let _ = self.client.request(DaemonRequest::ToggleStarSong(id)).await;
                }
                return Ok(());
            }
            KeyCode::Char('m')
                if state.client.artists.focus == 0
                    && !state.client.artists.filter.is_empty()
                    && state.client.artists.search_results.is_some() =>
            {
                let tree_items = build_tree_items(&state);
                let song_id = state
                    .client
                    .artists
                    .selected_index
                    .and_then(|idx| tree_items.get(idx))
                    .and_then(|item| match item {
                        TreeItem::Song { song } => Some(song.id.clone()),
                        _ => None,
                    });
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

    async fn collect_songs_for(
        &mut self,
        item: &crate::ui::pages::library::TreeItem,
    ) -> Vec<crate::subsonic::models::Child> {
        use crate::ui::pages::library::TreeItem;
        match item {
            TreeItem::Song { song } => vec![song.clone()],
            TreeItem::Album { album } => self.load_album(&album.id).await,
            TreeItem::Artist { artist, .. } => {
                let albums_resp = self
                    .client
                    .request(DaemonRequest::LoadArtist(artist.id.clone()))
                    .await;
                let albums = match albums_resp {
                    Ok(crate::ipc::DaemonResponse::ArtistAlbums(a)) => a,
                    _ => Vec::new(),
                };
                let mut all = Vec::new();
                for album in albums {
                    all.extend(self.load_album(&album.id).await);
                }
                all
            }
        }
    }
}
