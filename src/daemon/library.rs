//! Daemon-side cache of Subsonic library data.

use std::collections::{HashMap, HashSet, VecDeque};

use serde::{Deserialize, Serialize};

use crate::subsonic::models::{Album, Artist, Child, Playlist};

pub const ALBUMS_CACHE_CAP: usize = 50;
pub const ALBUM_SONGS_CACHE_CAP: usize = 100;
pub const PLAYLIST_SONGS_CACHE_CAP: usize = 50;
pub const COVER_ART_CACHE_CAP: usize = 64;

/// True LRU cache: most-recently-used end is the back of `order`. The
/// previous `HashMap::keys().next()` eviction was randomized and
/// thrashed hot keys.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LruCache<V> {
    map: HashMap<String, V>,
    order: VecDeque<String>,
}

impl<V: Clone> LruCache<V> {
    pub fn new() -> Self {
        Self {
            map: HashMap::new(),
            order: VecDeque::new(),
        }
    }
    /// Look up `key`. Returns `None` on miss; on hit, promotes the entry to MRU end before returning a borrow.
    ///
    /// ```
    /// use ferrosonic::daemon::library::LruCache;
    /// let mut c: LruCache<i32> = LruCache::new();
    /// assert!(c.get("missing").is_none());
    /// c.insert("k".to_string(), 7, 4);
    /// assert_eq!(c.get("k").copied(), Some(7));
    /// ```
    pub fn get(&mut self, key: &str) -> Option<&V> {
        if !self.map.contains_key(key) {
            return None;
        }
        // Touch to MRU end. Linear scan is fine for the small caps
        // used in this codebase (50-100).
        if let Some(pos) = self.order.iter().position(|k| k == key) {
            if let Some(k) = self.order.remove(pos) {
                self.order.push_back(k);
            }
        }
        self.map.get(key)
    }
    /// Insert `key=val` bounded by `cap`. If `key` already exists the value is replaced and the entry promoted to MRU. If inserting would exceed `cap`, evicts least-recently-used entries first.
    ///
    /// ```
    /// use ferrosonic::daemon::library::LruCache;
    /// let mut c: LruCache<i32> = LruCache::new();
    /// c.insert("a".to_string(), 1, 2);
    /// c.insert("b".to_string(), 2, 2);
    /// c.insert("c".to_string(), 3, 2);
    /// assert!(c.get("a").is_none(), "a should be evicted by cap=2");
    /// assert_eq!(c.get("b").copied(), Some(2));
    /// assert_eq!(c.get("c").copied(), Some(3));
    /// assert_eq!(c.len(), 2);
    /// ```
    pub fn insert(&mut self, key: String, val: V, cap: usize) {
        if self.map.contains_key(&key) {
            if let Some(pos) = self.order.iter().position(|k| k == &key) {
                if let Some(k) = self.order.remove(pos) {
                    self.order.push_back(k);
                }
            }
            self.map.insert(key, val);
            return;
        }
        while self.map.len() >= cap {
            if let Some(evict) = self.order.pop_front() {
                self.map.remove(&evict);
            } else {
                break;
            }
        }
        self.order.push_back(key.clone());
        self.map.insert(key, val);
    }
    /// Number of resident entries. Always equals `order.len()`.
    ///
    /// ```
    /// use ferrosonic::daemon::library::LruCache;
    /// let mut c: LruCache<()> = LruCache::new();
    /// assert_eq!(c.len(), 0);
    /// c.insert("x".to_string(), (), 4);
    /// assert_eq!(c.len(), 1);
    /// ```
    pub fn len(&self) -> usize {
        self.map.len()
    }
}

/// Compatibility shim while the rest of the codebase still uses a
/// bare `HashMap`. New code should use `LruCache` directly.
pub fn cache_insert<V: Clone>(
    map: &mut HashMap<String, V>,
    order: &mut VecDeque<String>,
    key: String,
    val: V,
    cap: usize,
) {
    if map.contains_key(&key) {
        if let Some(pos) = order.iter().position(|k| k == &key) {
            if let Some(k) = order.remove(pos) {
                order.push_back(k);
            }
        }
        map.insert(key, val);
        return;
    }
    while map.len() >= cap {
        if let Some(evict) = order.pop_front() {
            map.remove(&evict);
        } else {
            break;
        }
    }
    order.push_back(key.clone());
    map.insert(key, val);
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LibraryCache {
    pub starred_songs: Vec<Child>,
    /// O(1) lookup index over `starred_songs`. Rebuild via
    /// `rebuild_starred_index` after mutating `starred_songs`.
    #[serde(default)]
    pub starred_ids: HashSet<String>,
    pub random_songs: Vec<Child>,
    pub artists: Vec<Artist>,
    pub albums_cache: HashMap<String, Vec<Album>>,
    #[serde(default)]
    pub albums_cache_order: VecDeque<String>,
    pub album_songs_cache: HashMap<String, Vec<Child>>,
    #[serde(default)]
    pub album_songs_cache_order: VecDeque<String>,
    pub playlists: Vec<Playlist>,
    pub playlist_songs_cache: HashMap<String, Vec<Child>>,
    #[serde(default)]
    pub playlist_songs_cache_order: VecDeque<String>,
}

impl LibraryCache {
    /// Rebuild `starred_ids` from `starred_songs`. Call after any
    /// mutation to `starred_songs` so `song_is_starred` stays correct.
    pub fn rebuild_starred_index(&mut self) {
        self.starred_ids = self
            .starred_songs
            .iter()
            .filter(|s| s.starred.is_some())
            .map(|s| s.id.clone())
            .collect();
    }
}
