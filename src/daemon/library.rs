//! Library cache held by the daemon. Contains every Subsonic-fetched dataset
//! the TUI cares about: starred/random songs, the artist tree, per-artist
//! album lists, per-album song lists, playlists and per-playlist song lists.
//!
//! In phase 1 these are the canonical home for library data; per-page UI
//! state (e.g. `SongsState::songs`) is removed in favour of reading through
//! the cache. Subsequent phases populate the caches via daemon-side fetch
//! tasks instead of from per-handler calls.
//!
//! The per-id maps (`albums_cache`, `album_songs_cache`, `playlist_songs_cache`)
//! grow without bound today. Phase 8 caps them or moves to LRU.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::subsonic::models::{Album, Artist, Child, Playlist};

/// Max entries kept in each per-id cache. When over the cap, oldest
/// insertions are evicted to bound memory. HashMap iteration order is
/// not strict FIFO but is acceptable for a "stop unbounded growth" cap.
pub const ALBUMS_CACHE_CAP: usize = 50;
pub const ALBUM_SONGS_CACHE_CAP: usize = 100;
pub const PLAYLIST_SONGS_CACHE_CAP: usize = 50;

/// Insert into a HashMap cache, evicting one arbitrary entry first if
/// at capacity. Returns the cache unchanged when the key already exists
/// (refreshes the value).
pub fn cache_insert<V>(map: &mut HashMap<String, V>, key: String, val: V, cap: usize) {
    if !map.contains_key(&key) && map.len() >= cap {
        if let Some(evict_key) = map.keys().next().cloned() {
            map.remove(&evict_key);
        }
    }
    map.insert(key, val);
}

/// Library data fetched from the Subsonic server. Owned by the daemon.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LibraryCache {
    /// All starred songs (Songs page, "Starred" filter).
    pub starred_songs: Vec<Child>,
    /// Random song roll (Songs page, "Random" filter).
    pub random_songs: Vec<Child>,
    /// All artists (Artists page tree, left pane).
    pub artists: Vec<Artist>,
    /// Albums per artist id, populated lazily when the user expands an artist.
    pub albums_cache: HashMap<String, Vec<Album>>,
    /// Songs per album id, populated lazily when the user selects an album.
    /// (Empty in phase 1; populated by daemon-side fetches in later phases.)
    #[allow(dead_code)]
    pub album_songs_cache: HashMap<String, Vec<Child>>,
    /// All playlists (Playlists page, left pane).
    pub playlists: Vec<Playlist>,
    /// Songs per playlist id, populated lazily when the user selects a playlist.
    /// (Empty in phase 1; populated by daemon-side fetches in later phases.)
    #[allow(dead_code)]
    pub playlist_songs_cache: HashMap<String, Vec<Child>>,
}
