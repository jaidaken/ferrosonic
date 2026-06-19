//! Subsonic delegate loaders: album, playlist, search, cover art.

use std::sync::Arc;

use tracing::error;

use crate::daemon::core::DaemonCore;
use crate::ipc::protocol::DaemonEvent;

impl DaemonCore {
    /// Fetch an album's songs into the cache; empty Vec on failure.
    pub async fn load_album_songs(
        self: &Arc<Self>,
        album_id: &str,
    ) -> Vec<crate::subsonic::models::Child> {
        // Serve cached songs so hovering rows in the tree does not re-hit the
        // network on every keystroke; the cache tracks star changes already.
        if let Some(songs) = self
            .state
            .read()
            .await
            .library
            .album_songs_cache
            .get(album_id)
        {
            return songs.clone();
        }
        let Some(client) = self.subsonic.read().await.clone() else {
            return Vec::new();
        };
        match client.get_album(album_id).await {
            Ok((_album, songs)) => {
                {
                    let mut state = self.state.write().await;
                    let lib = &mut state.library;
                    crate::daemon::library::cache_insert(
                        &mut lib.album_songs_cache,
                        &mut lib.album_songs_cache_order,
                        album_id.to_string(),
                        songs.clone(),
                        crate::daemon::library::ALBUM_SONGS_CACHE_CAP,
                    );
                }
                self.emit(DaemonEvent::AlbumSongsChanged {
                    album_id: album_id.to_string(),
                    songs: songs.clone(),
                });
                songs
            }
            Err(e) => {
                error!("Failed to load album songs: {}", e);
                self.emit(DaemonEvent::Notification {
                    message: format!("Failed to load album: {}", e),
                    is_error: true,
                });
                Vec::new()
            }
        }
    }

    /// Server-side search; empty results on failure or no server.
    pub async fn search(
        self: &Arc<Self>,
        query: &str,
        artist_count: u32,
        album_count: u32,
        song_count: u32,
    ) -> crate::subsonic::models::SearchResult3 {
        let Some(client) = self.subsonic.read().await.clone() else {
            return Default::default();
        };
        match client
            .search3(query, artist_count, album_count, song_count)
            .await
        {
            Ok(r) => r,
            Err(e) => {
                error!("search3 failed: {}", e);
                Default::default()
            }
        }
    }

    /// Fetch a playlist's songs into the cache; empty Vec on failure.
    pub async fn load_playlist_songs(
        self: &Arc<Self>,
        playlist_id: &str,
    ) -> Vec<crate::subsonic::models::Child> {
        let Some(client) = self.subsonic.read().await.clone() else {
            return Vec::new();
        };
        match client.get_playlist(playlist_id).await {
            Ok((_pl, songs)) => {
                {
                    let mut state = self.state.write().await;
                    let lib = &mut state.library;
                    crate::daemon::library::cache_insert(
                        &mut lib.playlist_songs_cache,
                        &mut lib.playlist_songs_cache_order,
                        playlist_id.to_string(),
                        songs.clone(),
                        crate::daemon::library::PLAYLIST_SONGS_CACHE_CAP,
                    );
                }
                self.emit(DaemonEvent::PlaylistSongsChanged {
                    playlist_id: playlist_id.to_string(),
                    songs: songs.clone(),
                });
                songs
            }
            Err(e) => {
                error!("Failed to load playlist songs: {}", e);
                self.emit(DaemonEvent::Notification {
                    message: format!("Failed to load playlist: {}", e),
                    is_error: true,
                });
                Vec::new()
            }
        }
    }

    /// Returns empty on error so the caller renders no art.
    pub async fn get_cover_art(self: &Arc<Self>, id: &str, size: u32) -> Vec<u8> {
        let key = format!("{}@{}", id, size);
        {
            let mut cache = self.cover_art_cache.write().await;
            if let Some(bytes) = cache.get(&key) {
                return bytes.clone();
            }
        }
        let Some(client) = self.subsonic.read().await.clone() else {
            return Vec::new();
        };
        match client.get_cover_art(id, size).await {
            Ok(bytes) => {
                let mut cache = self.cover_art_cache.write().await;
                cache.insert(
                    key,
                    bytes.clone(),
                    crate::daemon::library::COVER_ART_CACHE_CAP,
                );
                bytes
            }
            Err(e) => {
                error!("get_cover_art failed for {}: {}", id, e);
                Vec::new()
            }
        }
    }

    /// Drop all cached cover art so the next fetch re-pulls from the server,
    /// picking up artwork changed there. Called on a library refresh (which the
    /// TUI runs at startup) and whenever the queue is replaced.
    pub(super) async fn clear_cover_cache(&self) {
        self.cover_art_cache.write().await.clear();
    }
}
