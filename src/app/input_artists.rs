use crossterm::event::{self, KeyCode};
use tracing::{error, info};

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
                        if let Some(client) = self.subsonic_client().await {
                            if let Ok((_album, songs)) = client.get_album(&album_id).await {
                                let mut state = self.state.write().await;
                                state.client.artists.songs = songs;
                                state.client.artists.selected_song = Some(0);
                            }
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
                        if let Some(client) = self.subsonic_client().await {
                            if let Ok((_album, songs)) = client.get_album(&album_id).await {
                                let mut state = self.state.write().await;
                                state.client.artists.songs = songs;
                                state.client.artists.selected_song = Some(0);
                            }
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

                                    if let Some(client) = self.subsonic_client().await {
                                        match client.get_artist(&artist_id).await {
                                            Ok((_artist, albums)) => {
                                                let mut artists_songs: Vec<_> = Vec::new();

                                                for (_i, album) in albums.into_iter().enumerate() {
                                                    match client.get_album(&album.id).await {
                                                        Ok((_album, songs)) => {
                                                            artists_songs.extend(songs);
                                                        }
                                                        Err(e) => {
                                                            // Skip failed album and shuffle the
                                                            // rest. Could be handled better.
                                                            error!("Failed to load: {}", e);
                                                        }
                                                    }
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
                                                let mut state = self.state.write().await;

                                                state.daemon.queue.clear();
                                                state.daemon.queue.extend(artists_songs);
                                                state.daemon.queue_position = Some(0);

                                                state.client.notify(format!(
                                                    "Shuffling {} songs by {}",
                                                    song_count, artist_name
                                                ));

                                                drop(state);

                                                return self.client.request(DaemonRequest::PlayQueueIndex(0)).await.map(|_| ()).map_err(Error::from);
                                            }
                                            Err(e) => {
                                                let mut state = self.state.write().await;
                                                state.client
                                                    .notify_error(format!("Failed to load: {}", e));
                                            }
                                        }
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
                                                    return Ok(());
                                                }

                                                let mut shuffled_songs: Vec<_> = Vec::from(songs);
                                                shuffled_songs.shuffle(&mut thread_rng());

                                                let mut state = self.state.write().await;

                                                state.daemon.queue.clear();
                                                state.daemon.queue.extend(shuffled_songs);
                                                state.daemon.queue_position = Some(0);

                                                state.client.notify(format!("Shuffling {}", album_name));

                                                drop(state);

                                                return self.client.request(DaemonRequest::PlayQueueIndex(0)).await.map(|_| ()).map_err(Error::from);
                                            }
                                            Err(e) => {
                                                let mut state = self.state.write().await;
                                                state.client
                                                    .notify_error(format!("Failed to load: {}", e));
                                            }
                                        }
                                    }
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
                                        if let Some(client) = self.subsonic_client().await {
                                            match client.get_artist(&artist_id).await {
                                                Ok((_artist, albums)) => {
                                                    let mut state = self.state.write().await;
                                                    let count = albums.len();
                                                    state
                                                        .daemon
                                                        .library
                                                        .albums_cache
                                                        .insert(artist_id.clone(), albums);
                                                    state.client.artists.expanded.insert(artist_id);
                                                    info!(
                                                        "Loaded {} albums for {}",
                                                        count, artist_name
                                                    );
                                                }
                                                Err(e) => {
                                                    let mut state = self.state.write().await;
                                                    state.client.notify_error(format!(
                                                        "Failed to load: {}",
                                                        e
                                                    ));
                                                }
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

                                    if let Some(client) = self.subsonic_client().await {
                                        match client.get_album(&album_id).await {
                                            Ok((_album, songs)) => {
                                                if songs.is_empty() {
                                                    let mut state = self.state.write().await;
                                                    state.client.notify_error("Album has no songs");
                                                    return Ok(());
                                                }

                                                let first_song = songs[0].clone();
                                                let stream_url =
                                                    client.get_stream_url(&first_song.id);

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
                                                state.daemon.now_playing.duration =
                                                    first_song.duration.unwrap_or(0) as f64;
                                                state.daemon.now_playing.sample_rate = None;
                                                state.daemon.now_playing.bit_depth = None;
                                                state.daemon.now_playing.format = None;
                                                state.daemon.now_playing.channels = None;
                                                state.client.notify(format!(
                                                    "Playing album: {} ({} songs)",
                                                    album_name, count
                                                ));
                                                drop(state);

                                                match stream_url {
                                                    Ok(url) => {
                                                        self.core.play_url_now(&url).await;
                                                    }
                                                    Err(e) => {
                                                        error!("Failed to get stream URL: {}", e);
                                                    }
                                                }
                                            }
                                            Err(e) => {
                                                let mut state = self.state.write().await;
                                                state.client.notify_error(format!(
                                                    "Failed to load album: {}",
                                                    e
                                                ));
                                            }
                                        }
                                    }
                                    return Ok(());
                                }
                            }
                        }
                    }
                } else {
                    // Play selected song from current position
                    if let Some(idx) = state.client.artists.selected_song {
                        if idx < state.client.artists.songs.len() {
                            let song = state.client.artists.songs[idx].clone();
                            let songs = state.client.artists.songs.clone();
                            state.daemon.queue.clear();
                            state.daemon.queue.extend(songs);
                            state.daemon.queue_position = Some(idx);
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
                                match client.get_stream_url(&song.id) {
                                    Ok(url) => {
                                        self.core.play_url_now(&url).await;
                                    }
                                    Err(e) => {
                                        error!("Failed to get stream URL: {}", e);
                                    }
                                }
                            }
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
                            state.daemon.queue.push(song);
                            state.client.notify(format!("Added to queue: {}", title));
                        }
                    }
                } else if !state.client.artists.songs.is_empty() {
                    let count = state.client.artists.songs.len();
                    let songs = state.client.artists.songs.clone();
                    state.daemon.queue.extend(songs);
                    state.client.notify(format!("Added {} songs to queue", count));
                }
            }
            KeyCode::Char('n') => {
                let insert_pos = state.daemon.queue_position.map(|p| p + 1).unwrap_or(0);
                if state.client.artists.focus == 1 {
                    if let Some(idx) = state.client.artists.selected_song {
                        if let Some(song) = state.client.artists.songs.get(idx).cloned() {
                            let title = song.title.clone();
                            state.daemon.queue.insert(insert_pos, song);
                            state.client.notify(format!("Playing next: {}", title));
                        }
                    }
                } else if !state.client.artists.songs.is_empty() {
                    let count = state.client.artists.songs.len();
                    let songs: Vec<_> = state.client.artists.songs.to_vec();
                    for (i, song) in songs.into_iter().enumerate() {
                        state.daemon.queue.insert(insert_pos + i, song);
                    }
                    state.client.notify(format!("Playing {} songs next", count));
                }
            }
            _ => {}
        }

        Ok(())
    }
}
