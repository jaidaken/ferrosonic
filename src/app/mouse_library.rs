use crate::error::Error;

use super::{App, AppState, DaemonRequest, EnqueueMode, LayoutAreas};

impl App {
    // Cohesive single match/render; splitting would fragment one logical unit.
    #[allow(clippy::too_many_lines)]
    // significant_drop_tightening: guards here are borrow-bound (used via &mut field); the suggested early-drop fails to compile.
    #[allow(clippy::significant_drop_tightening)]
    pub(super) async fn handle_library_click(
        &mut self,
        x: u16,
        y: u16,
        layout: &LayoutAreas,
    ) -> Result<(), Error> {
        use crate::ui::pages::library::{build_tree_items, TreeItem};

        let ds = self.daemon_state.read().await;

        let mut cs = self.client_state.write().await;

        let state = AppState {
            daemon: &ds,
            client: &mut cs,
        };
        let left = layout.content_left.unwrap_or(layout.content);
        let right = layout.content_right.unwrap_or(layout.content);

        if x >= left.x && x < left.x + left.width && y >= left.y && y < left.y + left.height {
            let row_in_viewport = y.saturating_sub(left.y + 1) as usize;
            let item_index = state.client.artists.tree_scroll_offset + row_in_viewport;
            let tree_items = build_tree_items(&state);

            if item_index < tree_items.len() {
                let was_selected = state.client.artists.selected_index == Some(item_index);
                state.client.artists.focus = 0;
                state.client.artists.selected_index = Some(item_index);

                let is_second_click = was_selected
                    && self.last_click.is_some_and(|(lx, ly, t)| {
                        lx == x && ly == y && t.elapsed().as_millis() < 500
                    });

                if is_second_click {
                    match &tree_items[item_index] {
                        TreeItem::Artist {
                            artist, expanded, ..
                        } => {
                            let artist_id = artist.id.clone();
                            let artist_name = artist.name.clone();
                            let was_expanded = *expanded;

                            if was_expanded {
                                state.client.artists.expanded.remove(&artist_id);
                            } else if !state.daemon.library.albums_cache.contains_key(&artist_id) {
                                let _ = state;
                                drop(cs);
                                drop(ds);
                                let albums_resp = self
                                    .client
                                    .request(DaemonRequest::LoadArtist(artist_id.clone()))
                                    .await;
                                if let Ok(crate::ipc::DaemonResponse::ArtistAlbums(_)) = albums_resp
                                {
                                    // Cache + AlbumsChanged already emitted by daemon.
                                    let ds = self.daemon_state.read().await;
                                    let mut cs = self.client_state.write().await;
                                    let state = AppState {
                                        daemon: &ds,
                                        client: &mut cs,
                                    };
                                    state.client.artists.expanded.insert(artist_id);
                                    tracing::info!("Loaded albums for {}", artist_name);
                                } else {
                                    let ds = self.daemon_state.read().await;
                                    let mut cs = self.client_state.write().await;
                                    let state = AppState {
                                        daemon: &ds,
                                        client: &mut cs,
                                    };
                                    state.client.notify_error("Failed to load artist");
                                }
                                self.last_click = Some((x, y, std::time::Instant::now()));
                                return Ok(());
                            } else {
                                state.client.artists.expanded.insert(artist_id);
                            }
                        }
                        TreeItem::Album { album } => {
                            let album_id = album.id.clone();
                            let album_name = album.name.clone();
                            let _ = state;
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
                                self.last_click = Some((x, y, std::time::Instant::now()));
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
                                state.client.artists.songs.clone_from(&songs);
                                state.client.artists.selected_song = Some(0);
                                state.client.artists.focus = 1;
                                state
                                    .client
                                    .notify(format!("Playing album: {album_name} ({count} songs)"));
                            }
                            let _ = self
                                .client
                                .request(DaemonRequest::EnqueueSongs {
                                    songs,
                                    mode: EnqueueMode::Replace { play_from: Some(0) },
                                })
                                .await;
                            self.last_click = Some((x, y, std::time::Instant::now()));
                            return Ok(());
                        }
                        TreeItem::Song { song } => {
                            let song = song.clone();
                            let title = song.title.clone();
                            let _ = state;
                            drop(cs);
                            drop(ds);
                            {
                                let ds = self.daemon_state.read().await;
                                let mut cs = self.client_state.write().await;
                                let state = AppState {
                                    daemon: &ds,
                                    client: &mut cs,
                                };
                                state.client.notify(format!("Playing: {title}"));
                            }
                            let _ = self
                                .client
                                .request(DaemonRequest::EnqueueSongs {
                                    songs: vec![song],
                                    mode: EnqueueMode::Replace { play_from: Some(0) },
                                })
                                .await;
                            self.last_click = Some((x, y, std::time::Instant::now()));
                            return Ok(());
                        }
                        TreeItem::ArtistLabel { .. } | TreeItem::AlbumLabel { .. } => {}
                    }
                } else if let TreeItem::Album { album } = &tree_items[item_index] {
                    let album_id = album.id.clone();
                    let _ = state;
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
                    self.last_click = Some((x, y, std::time::Instant::now()));
                    return Ok(());
                }
            }
        } else if x >= right.x
            && x < right.x + right.width
            && y >= right.y
            && y < right.y + right.height
        {
            let row_in_viewport = y.saturating_sub(right.y + 1) as usize;
            let item_index = state.client.artists.song_scroll_offset + row_in_viewport;

            if item_index < state.client.artists.songs.len() {
                let was_selected = state.client.artists.selected_song == Some(item_index);
                state.client.artists.focus = 1;
                state.client.artists.selected_song = Some(item_index);

                let is_second_click = was_selected
                    && self.last_click.is_some_and(|(lx, ly, t)| {
                        lx == x && ly == y && t.elapsed().as_millis() < 500
                    });

                if is_second_click {
                    let songs = state.client.artists.songs.clone();
                    if let Some(song) = songs.get(item_index) {
                        state.client.notify(format!("Playing: {}", song.title));
                    }
                    let _ = state;
                    drop(cs);
                    drop(ds);
                    let _ = self
                        .client
                        .request(DaemonRequest::EnqueueSongs {
                            songs,
                            mode: EnqueueMode::Replace {
                                play_from: Some(item_index),
                            },
                        })
                        .await;
                    self.last_click = Some((x, y, std::time::Instant::now()));
                    return Ok(());
                }
            }
        }

        self.last_click = Some((x, y, std::time::Instant::now()));
        Ok(())
    }
}
