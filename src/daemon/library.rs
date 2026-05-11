//! Daemon-side cache of Subsonic library data.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::subsonic::models::{Album, Artist, Child, Playlist};

pub const ALBUMS_CACHE_CAP: usize = 50;
pub const ALBUM_SONGS_CACHE_CAP: usize = 100;
pub const PLAYLIST_SONGS_CACHE_CAP: usize = 50;

/// Insert with eviction. HashMap iteration order is not strict FIFO
/// but is acceptable for bounding memory growth.
pub fn cache_insert<V>(map: &mut HashMap<String, V>, key: String, val: V, cap: usize) {
    if !map.contains_key(&key) && map.len() >= cap {
        if let Some(evict_key) = map.keys().next().cloned() {
            map.remove(&evict_key);
        }
    }
    map.insert(key, val);
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LibraryCache {
    pub starred_songs: Vec<Child>,
    pub random_songs: Vec<Child>,
    pub artists: Vec<Artist>,
    pub albums_cache: HashMap<String, Vec<Album>>,
    #[allow(dead_code)]
    pub album_songs_cache: HashMap<String, Vec<Child>>,
    pub playlists: Vec<Playlist>,
    #[allow(dead_code)]
    pub playlist_songs_cache: HashMap<String, Vec<Child>>,
}
