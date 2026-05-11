use crate::error::Error;

use super::*;

impl App {
    pub(super) async fn handle_playlists_click(
        &mut self,
        x: u16,
        y: u16,
        layout: &LayoutAreas,
    ) -> Result<(), Error> {
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
                    let playlist = state.daemon.library.playlists[item_index].clone();
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
                    state.client.playlists.selected_song = if count > 0 { Some(0) } else { None };
                    state.client.playlists.focus = 1;
                    state.client.notify(format!(
                        "Loaded playlist: {} ({} songs)",
                        playlist_name, count
                    ));
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
                    let songs = state.client.playlists.songs.clone();
                    drop(state);
                    drop(cs);
                    drop(ds);
                    self.last_click = Some((x, y, std::time::Instant::now()));
                    return self
                        .client
                        .request(DaemonRequest::EnqueueSongs {
                            songs,
                            mode: EnqueueMode::Replace {
                                play_from: Some(item_index),
                            },
                        })
                        .await
                        .map(|_| ())
                        .map_err(Error::from);
                }
            }
        }

        self.last_click = Some((x, y, std::time::Instant::now()));
        Ok(())
    }
}
