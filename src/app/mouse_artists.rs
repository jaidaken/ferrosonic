use crate::error::Error;

use super::*;

impl App {
    /// Handle click on artists page
    pub(super) async fn handle_artists_click(
        &mut self,
        x: u16,
        y: u16,
        layout: &LayoutAreas,
    ) -> Result<(), Error> {
        use crate::ui::pages::artists::{build_tree_items, TreeItem};

        let mut state = self.state.write().await;
        let left = layout.content_left.unwrap_or(layout.content);
        let right = layout.content_right.unwrap_or(layout.content);

        if x >= left.x && x < left.x + left.width && y >= left.y && y < left.y + left.height {
            // Tree pane click — account for border (1 row top)
            let row_in_viewport = y.saturating_sub(left.y + 1) as usize;
            let item_index = state.client.artists.tree_scroll_offset + row_in_viewport;
            let tree_items = build_tree_items(&state);

            if item_index < tree_items.len() {
                let was_selected = state.client.artists.selected_index == Some(item_index);
                state.client.artists.focus = 0;
                state.client.artists.selected_index = Some(item_index);

                // Second click = activate (same as Enter)
                let is_second_click = was_selected
                    && self.last_click.is_some_and(|(lx, ly, t)| {
                        lx == x && ly == y && t.elapsed().as_millis() < 500
                    });

                if is_second_click {
                    // Activate: expand/collapse artist, or play album
                    match &tree_items[item_index] {
                        TreeItem::Artist { artist, expanded } => {
                            let artist_id = artist.id.clone();
                            let artist_name = artist.name.clone();
                            let was_expanded = *expanded;

                            if was_expanded {
                                state.client.artists.expanded.remove(&artist_id);
                            } else if !state.daemon.library.albums_cache.contains_key(&artist_id) {
                                drop(state);
                                if let Some(client) = self.subsonic_client().await {
                                    match client.get_artist(&artist_id).await {
                                        Ok((_artist, albums)) => {
                                            let mut state = self.state.write().await;
                                            let count = albums.len();
                                            state.daemon.library.albums_cache.insert(artist_id.clone(), albums);
                                            state.client.artists.expanded.insert(artist_id);
                                            tracing::info!("Loaded {} albums for {}", count, artist_name);
                                        }
                                        Err(e) => {
                                            let mut state = self.state.write().await;
                                            state.client.notify_error(format!("Failed to load: {}", e));
                                        }
                                    }
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
                            drop(state);

                            if let Some(client) = self.subsonic_client().await {
                                match client.get_album(&album_id).await {
                                    Ok((_album, songs)) => {
                                        if songs.is_empty() {
                                            let mut state = self.state.write().await;
                                            state.client.notify_error("Album has no songs");
                                            self.last_click = Some((x, y, std::time::Instant::now()));
                                            return Ok(());
                                        }

                                        let first_song = songs[0].clone();
                                        let stream_url = client.get_stream_url(&first_song.id);

                                        let mut state = self.state.write().await;
                                        let count = songs.len();
                                        state.daemon.queue.clear();
                                        state.daemon.queue.extend(songs.clone());
                                        state.daemon.queue_position = Some(0);
                                        state.client.artists.songs = songs;
                                        state.client.artists.selected_song = Some(0);
                                        state.client.artists.focus = 1;
                                        state.daemon.now_playing.song = Some(first_song.clone());
                                        state.daemon.now_playing.state = PlaybackState::Playing;
                                        state.daemon.now_playing.position = 0.0;
                                        state.daemon.now_playing.duration = first_song.duration.unwrap_or(0) as f64;
                                        state.daemon.now_playing.sample_rate = None;
                                        state.daemon.now_playing.bit_depth = None;
                                        state.daemon.now_playing.format = None;
                                        state.daemon.now_playing.channels = None;
                                        state.client.notify(format!("Playing album: {} ({} songs)", album_name, count));
                                        drop(state);

                                        if let Ok(url) = stream_url {
                                            self.core.play_url_now(&url).await;
                                        }
                                    }
                                    Err(e) => {
                                        let mut state = self.state.write().await;
                                        state.client.notify_error(format!("Failed to load album: {}", e));
                                    }
                                }
                            }
                            self.last_click = Some((x, y, std::time::Instant::now()));
                            return Ok(());
                        }
                    }
                } else {
                    // First click on album: preview songs in right pane
                    if let TreeItem::Album { album } = &tree_items[item_index] {
                        let album_id = album.id.clone();
                        drop(state);
                        if let Some(client) = self.subsonic_client().await {
                            if let Ok((_album, songs)) = client.get_album(&album_id).await {
                                let mut state = self.state.write().await;
                                state.client.artists.songs = songs;
                                state.client.artists.selected_song = Some(0);
                            }
                        }
                        self.last_click = Some((x, y, std::time::Instant::now()));
                        return Ok(());
                    }
                }
            }
        } else if x >= right.x && x < right.x + right.width && y >= right.y && y < right.y + right.height {
            // Songs pane click
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
                    // Play selected song
                    let song = state.client.artists.songs[item_index].clone();
                    let songs = state.client.artists.songs.clone();
                    state.daemon.queue.clear();
                    state.daemon.queue.extend(songs);
                    state.daemon.queue_position = Some(item_index);
                    state.daemon.now_playing.song = Some(song.clone());
                    state.daemon.now_playing.state = PlaybackState::Playing;
                    state.daemon.now_playing.position = 0.0;
                    state.daemon.now_playing.duration = song.duration.unwrap_or(0) as f64;
                    state.daemon.now_playing.sample_rate = None;
                    state.daemon.now_playing.bit_depth = None;
                    state.daemon.now_playing.format = None;
                    state.daemon.now_playing.channels = None;
                    state.client.notify(format!("Playing: {}", song.title));
                    drop(state);

                    if let Some(client) = self.subsonic_client().await {
                        if let Ok(url) = client.get_stream_url(&song.id) {
                            self.core.play_url_now(&url).await;
                        }
                    }
                    self.last_click = Some((x, y, std::time::Instant::now()));
                    return Ok(());
                }
            }
        }

        self.last_click = Some((x, y, std::time::Instant::now()));
        Ok(())
    }
}
