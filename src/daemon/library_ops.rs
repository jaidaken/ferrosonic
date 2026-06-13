//! Library cache mutation: refresh + star toggle + artist load.

use std::sync::Arc;

use tracing::{debug, error, info, warn};

use crate::daemon::core::DaemonCore;
use crate::daemon::state::DaemonState;
use crate::error::Error;
use crate::ipc::protocol::DaemonEvent;

impl DaemonCore {
    /// Re-fetch starred songs and broadcast the new list.
    pub async fn refresh_starred(self: &Arc<Self>) {
        let Some(client) = self.subsonic.read().await.clone() else {
            return;
        };
        let gen_at_start = self.config_gen.load(std::sync::atomic::Ordering::Acquire);
        match client.get_starred_songs().await {
            Ok(songs) => {
                if self.config_gen_changed(gen_at_start) {
                    debug!("refresh_starred: config changed mid-request, discarding");
                    return;
                }
                let mut state = self.state.write().await;
                state.library.starred_songs = songs.clone();
                state.library.rebuild_starred_index();
                drop(state);
                self.emit(DaemonEvent::StarredChanged(songs));
                self.bump_library_version();
            }
            Err(e) => {
                error!("Failed to load starred songs: {}", e);
                self.emit(DaemonEvent::Notification {
                    message: format!("Failed to load starred songs: {}", e),
                    is_error: true,
                });
            }
        }
    }

    /// Re-fetch the random-songs batch and broadcast the new list.
    pub async fn refresh_random(self: &Arc<Self>) {
        let Some(client) = self.subsonic.read().await.clone() else {
            return;
        };
        let gen_at_start = self.config_gen.load(std::sync::atomic::Ordering::Acquire);
        match client.get_random_songs().await {
            Ok(songs) => {
                if self.config_gen_changed(gen_at_start) {
                    debug!("refresh_random: config changed mid-request, discarding");
                    return;
                }
                let mut state = self.state.write().await;
                state.library.random_songs = songs.clone();
                drop(state);
                self.emit(DaemonEvent::RandomChanged(songs));
                self.bump_library_version();
            }
            Err(e) => {
                error!("Failed to load random songs: {}", e);
                self.emit(DaemonEvent::Notification {
                    message: format!("Failed to load random songs: {}", e),
                    is_error: true,
                });
            }
        }
    }

    /// Re-fetch the artist index and broadcast the new list.
    pub async fn refresh_artists(self: &Arc<Self>) {
        let Some(client) = self.subsonic.read().await.clone() else {
            return;
        };
        let gen_at_start = self.config_gen.load(std::sync::atomic::Ordering::Acquire);
        match client.get_artists().await {
            Ok(artists) => {
                if self.config_gen_changed(gen_at_start) {
                    debug!("refresh_artists: config changed mid-request, discarding");
                    return;
                }
                let mut state = self.state.write().await;
                let count = artists.len();
                state.library.artists = artists.clone();
                drop(state);
                info!("Loaded {} artists", count);
                self.emit(DaemonEvent::ArtistsChanged(artists));
                self.bump_library_version();
            }
            Err(e) => {
                error!("Failed to load artists: {}", e);
                self.emit(DaemonEvent::Notification {
                    message: format!("Failed to load artists: {}", e),
                    is_error: true,
                });
            }
        }
    }

    /// Re-fetch the playlist list and broadcast the new list.
    pub async fn refresh_playlists(self: &Arc<Self>) {
        let Some(client) = self.subsonic.read().await.clone() else {
            return;
        };
        let gen_at_start = self.config_gen.load(std::sync::atomic::Ordering::Acquire);
        match client.get_playlists().await {
            Ok(playlists) => {
                if self.config_gen_changed(gen_at_start) {
                    debug!("refresh_playlists: config changed mid-request, discarding");
                    return;
                }
                let mut state = self.state.write().await;
                let count = playlists.len();
                state.library.playlists = playlists.clone();
                drop(state);
                info!("Loaded {} playlists", count);
                self.emit(DaemonEvent::PlaylistsChanged(playlists));
                self.bump_library_version();
            }
            Err(e) => {
                error!("Failed to load playlists: {}", e);
            }
        }
    }

    /// Star or unstar `song_id`; returns the new starred state.
    pub async fn toggle_star_song(self: &Arc<Self>, song_id: &str) -> Result<bool, Error> {
        let Some(client) = self.subsonic.read().await.clone() else {
            return Err(Error::Subsonic(crate::error::SubsonicError::Api {
                code: 0,
                message: "Subsonic client not configured".to_string(),
            }));
        };

        let gen_at_start = self.config_gen.load(std::sync::atomic::Ordering::Acquire);
        // R1: read currently_starred under the same write lock that will commit the toggle so a concurrent toggle cannot flip the bit between read and the cache mutation below.
        let (currently_starred, new_starred) = {
            let mut state = self.state.write().await;
            let was = song_is_starred(&state, song_id);
            let now = !was;
            apply_star_to_cached(&mut state, song_id, now);
            (was, now)
        };

        let rpc_result = if currently_starred {
            client.unstar_song(song_id).await
        } else {
            client.star_song(song_id).await
        };
        if let Err(e) = rpc_result {
            let mut state = self.state.write().await;
            apply_star_to_cached(&mut state, song_id, currently_starred);
            return Err(Error::Subsonic(e));
        }

        let refreshed = match client.get_starred_songs().await {
            Ok(list) => Some(list),
            Err(e) => {
                warn!("Post-toggle starred refresh failed: {}", e);
                None
            }
        };
        let stale = self.config_gen_changed(gen_at_start);

        let new_list = {
            let mut state = self.state.write().await;
            if let Some(list) = refreshed.as_ref() {
                if !stale {
                    state.library.starred_songs = list.clone();
                    state.library.rebuild_starred_index();
                }
            }
            state.library.starred_songs.clone()
        };
        self.emit(DaemonEvent::SongStarChanged {
            id: song_id.to_string(),
            starred: new_starred,
        });
        if refreshed.is_some() && !stale {
            self.emit(DaemonEvent::StarredChanged(new_list));
            self.bump_library_version();
        }
        Ok(new_starred)
    }

    /// Fetch one artist's albums into the cache and broadcast them.
    pub async fn load_artist(self: &Arc<Self>, artist_id: &str) {
        let Some(client) = self.subsonic.read().await.clone() else {
            return;
        };
        match client.get_artist(artist_id).await {
            Ok((_artist, albums)) => {
                let mut state = self.state.write().await;
                let count = albums.len();
                let lib = &mut state.library;
                crate::daemon::library::cache_insert(
                    &mut lib.albums_cache,
                    &mut lib.albums_cache_order,
                    artist_id.to_string(),
                    albums.clone(),
                    crate::daemon::library::ALBUMS_CACHE_CAP,
                );
                drop(state);
                info!("Loaded {} albums for {}", count, artist_id);
                self.emit(DaemonEvent::AlbumsChanged {
                    artist_id: artist_id.to_string(),
                    albums,
                });
            }
            Err(e) => {
                error!("Failed to load albums: {}", e);
                self.emit(DaemonEvent::Notification {
                    message: format!("Failed to load albums: {}", e),
                    is_error: true,
                });
            }
        }
    }
}

fn song_is_starred(daemon: &DaemonState, song_id: &str) -> bool {
    if daemon.library.starred_ids.contains(song_id) {
        return true;
    }
    // Fallback scan: per-song starred marker may live in queue, random, album_songs, playlist_songs caches only.
    daemon
        .library
        .starred_songs
        .iter()
        .chain(daemon.queue.iter())
        .chain(daemon.library.random_songs.iter())
        .chain(daemon.library.album_songs_cache.values().flatten())
        .chain(daemon.library.playlist_songs_cache.values().flatten())
        .any(|s| s.id == song_id && s.starred.is_some())
}

fn apply_star_to_cached(daemon: &mut DaemonState, song_id: &str, starred: bool) {
    let marker = if starred { Some("1".to_string()) } else { None };
    let lists: [&mut Vec<crate::subsonic::models::Child>; 2] =
        [&mut daemon.queue, &mut daemon.library.random_songs];
    for list in lists {
        for song in list.iter_mut() {
            if song.id == song_id {
                song.starred = marker.clone();
            }
        }
    }
    for list in daemon.library.album_songs_cache.values_mut() {
        for song in list.iter_mut() {
            if song.id == song_id {
                song.starred = marker.clone();
            }
        }
    }
    for list in daemon.library.playlist_songs_cache.values_mut() {
        for song in list.iter_mut() {
            if song.id == song_id {
                song.starred = marker.clone();
            }
        }
    }
    if let Some(np) = daemon.now_playing.song.as_mut() {
        if np.id == song_id {
            np.starred = marker.clone();
        }
    }
    sync_starred_songs(daemon, song_id, starred, marker);
}

fn sync_starred_songs(
    daemon: &mut DaemonState,
    song_id: &str,
    starred: bool,
    marker: Option<String>,
) {
    if starred {
        daemon.library.starred_ids.insert(song_id.to_string());
        let already = daemon
            .library
            .starred_songs
            .iter()
            .any(|s| s.id == song_id);
        if !already {
            let source = daemon
                .queue
                .iter()
                .chain(daemon.library.random_songs.iter())
                .chain(daemon.library.album_songs_cache.values().flatten())
                .chain(daemon.library.playlist_songs_cache.values().flatten())
                .find(|s| s.id == song_id)
                .cloned();
            if let Some(mut s) = source {
                s.starred = marker;
                daemon.library.starred_songs.push(s);
            }
        } else {
            for s in daemon.library.starred_songs.iter_mut() {
                if s.id == song_id {
                    s.starred = marker.clone();
                }
            }
        }
    } else {
        daemon.library.starred_ids.remove(song_id);
        daemon.library.starred_songs.retain(|s| s.id != song_id);
    }
}
