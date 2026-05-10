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

use crate::subsonic::models::{Album, Artist, Child, Playlist};

/// Library data fetched from the Subsonic server. Owned by the daemon.
#[derive(Debug, Clone, Default)]
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
