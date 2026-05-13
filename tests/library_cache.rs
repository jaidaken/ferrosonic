//! daemon/library.rs: cache_insert eviction + LibraryCache defaults.

use ferrosonic::daemon::library::{cache_insert, LibraryCache, ALBUMS_CACHE_CAP};
use std::collections::{HashMap, VecDeque};

#[test]
fn cache_insert_adds_within_cap() {
    let mut m: HashMap<String, i32> = HashMap::new();
    let mut order: VecDeque<String> = VecDeque::new();
    cache_insert(&mut m, &mut order, "a".into(), 1, 10);
    cache_insert(&mut m, &mut order, "b".into(), 2, 10);
    assert_eq!(m.get("a"), Some(&1));
    assert_eq!(m.get("b"), Some(&2));
    assert_eq!(m.len(), 2);
}

#[test]
fn cache_insert_evicts_in_fifo_order_when_at_cap() {
    let mut m: HashMap<String, i32> = HashMap::new();
    let mut order: VecDeque<String> = VecDeque::new();
    for i in 0..ALBUMS_CACHE_CAP {
        cache_insert(
            &mut m,
            &mut order,
            format!("k{}", i),
            i as i32,
            ALBUMS_CACHE_CAP,
        );
    }
    assert_eq!(m.len(), ALBUMS_CACHE_CAP);
    // The oldest insert (k0) is the LRU end and must be evicted first.
    cache_insert(&mut m, &mut order, "overflow".into(), 999, ALBUMS_CACHE_CAP);
    assert_eq!(m.len(), ALBUMS_CACHE_CAP);
    assert_eq!(m.get("overflow"), Some(&999));
    assert_eq!(m.get("k0"), None, "oldest entry must evict first");
    assert_eq!(m.get("k1"), Some(&1));
}

#[test]
fn cache_insert_overwrite_existing_key_does_not_evict() {
    let mut m: HashMap<String, i32> = HashMap::new();
    let mut order: VecDeque<String> = VecDeque::new();
    for i in 0..5 {
        cache_insert(&mut m, &mut order, format!("k{}", i), i, 5);
    }
    cache_insert(&mut m, &mut order, "k2".into(), 9999, 5);
    assert_eq!(m.len(), 5);
    assert_eq!(m.get("k2"), Some(&9999));
}

#[test]
fn library_cache_default_is_empty() {
    let lc = LibraryCache::default();
    assert!(lc.starred_songs.is_empty());
    assert!(lc.random_songs.is_empty());
    assert!(lc.artists.is_empty());
    assert!(lc.albums_cache.is_empty());
    assert!(lc.playlists.is_empty());
    assert!(lc.starred_ids.is_empty());
}
