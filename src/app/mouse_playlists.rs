use crate::error::Error;

use super::*;

impl App {
    /// Handle click on playlists page
    pub(super) async fn handle_playlists_click(
        &mut self,
        x: u16,
        y: u16,
        layout: &LayoutAreas,
    ) -> Result<(), Error> {
        let mut state = self.state.write().await;
        let left = layout.content_left.unwrap_or(layout.content);
        let right = layout.content_right.unwrap_or(layout.content);

        if x >= left.x && x < left.x + left.width && y >= left.y && y < left.y + left.height {
            // Playlists pane
            let row_in_viewport = y.saturating_sub(left.y + 1) as usize;
            let item_index = state.client.playlists.playlist_scroll_offset + row_in_viewport;

            if item_index < state.daemon.library.playlists.len() {
                let was_selected = state.client.playlists.selected_playlist == Some(item_index);
                state.client.playlists.focus = 0;
                state.client.playlists.selected_playlist = Some(item_index);

                let is_second_click = was_selected
                    && self.last_click.is_some_and(|(lx, ly, t)| {
                        lx == x && ly == y && t.elapsed().as_millis() < 500
                    });

                if is_second_click {
                    // Load playlist songs (same as Enter)
                    let playlist = state.daemon.library.playlists[item_index].clone();
                    let playlist_id = playlist.id.clone();
                    let playlist_name = playlist.name.clone();
                    drop(state);

                    if let Some(ref client) = self.subsonic {
                        match client.get_playlist(&playlist_id).await {
                            Ok((_playlist, songs)) => {
                                let mut state = self.state.write().await;
                                let count = songs.len();
                                state.client.playlists.songs = songs;
                                state.client.playlists.selected_song = if count > 0 { Some(0) } else { None };
                                state.client.playlists.focus = 1;
                                state.client.notify(format!("Loaded playlist: {} ({} songs)", playlist_name, count));
                            }
                            Err(e) => {
                                let mut state = self.state.write().await;
                                state.client.notify_error(format!("Failed to load playlist: {}", e));
                            }
                        }
                    }
                    self.last_click = Some((x, y, std::time::Instant::now()));
                    return Ok(());
                }
            }
        } else if x >= right.x && x < right.x + right.width && y >= right.y && y < right.y + right.height {
            // Songs pane
            let row_in_viewport = y.saturating_sub(right.y + 1) as usize;
            let item_index = state.client.playlists.song_scroll_offset + row_in_viewport;

            if item_index < state.client.playlists.songs.len() {
                let was_selected = state.client.playlists.selected_song == Some(item_index);
                state.client.playlists.focus = 1;
                state.client.playlists.selected_song = Some(item_index);

                let is_second_click = was_selected
                    && self.last_click.is_some_and(|(lx, ly, t)| {
                        lx == x && ly == y && t.elapsed().as_millis() < 500
                    });

                if is_second_click {
                    // Play selected song from playlist
                    let songs = state.client.playlists.songs.clone();
                    state.daemon.queue.clear();
                    state.daemon.queue.extend(songs);
                    drop(state);
                    self.last_click = Some((x, y, std::time::Instant::now()));
                    return self.play_queue_position(item_index).await;
                }
            }
        }

        self.last_click = Some((x, y, std::time::Instant::now()));
        Ok(())
    }
}
