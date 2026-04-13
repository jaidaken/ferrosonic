use super::*;

impl App {
    pub async fn get_starred_songs(&mut self) {
        if let Some(ref client) = self.subsonic {
            match client.get_starred_songs().await {
                Ok(songs) => {
                    let mut state = self.state.write().await;
                    let count = songs.len();
                    state.songs.songs = songs;
                    if count > 0 {
                        state.songs.selected_index = Some(0);
                    } else {
                        state.songs.selected_index = None;
                    }
                }
                Err(e) => {
                    error!("Failed to load starred songs: {}", e);
                    let mut state = self.state.write().await;
                    state.notify_error(format!("Failed to load starred songs: {}", e));
                }
            }
        }
    }

    pub async fn get_random_songs(&mut self) {
        if let Some(ref client) = self.subsonic {
            match client.get_random_songs().await {
                Ok(songs) => {
                    let mut state = self.state.write().await;
                    let count = songs.len();
                    state.songs.songs = songs;
                    if count > 0 {
                        state.songs.selected_index = Some(0);
                    } else {
                        state.songs.selected_index = None;
                    }
                }
                Err(e) => {
                    error!("Failed to load random songs: {}", e);
                    let mut state = self.state.write().await;
                    state.notify_error(format!("Failed to load random songs: {}", e));
                }
            }
        }
    }

    pub async fn get_artists(&mut self) {
        if let Some(ref client) = self.subsonic {
            match client.get_artists().await {
                Ok(artists) => {
                    let mut state = self.state.write().await;
                    let count = artists.len();
                    state.artists.artists = artists;
                    if count > 0 {
                        state.artists.selected_index = Some(0);
                    }
                    info!("Loaded {} artists", count);
                }
                Err(e) => {
                    error!("Failed to load artists: {}", e);
                    let mut state = self.state.write().await;
                    state.notify_error(format!("Failed to load artists: {}", e));
                }
            }
        }
    }

    pub async fn get_playlists(&mut self) {
        if let Some(ref client) = self.subsonic {
            match client.get_playlists().await {
                Ok(playlists) => {
                    let mut state = self.state.write().await;
                    let count = playlists.len();
                    state.playlists.playlists = playlists;
                    info!("Loaded {} playlists", count);
                }
                Err(e) => {
                    error!("Failed to load playlists: {}", e);
                    // Don't show error for playlists if artists loaded
                }
            }
        }
    }
}
