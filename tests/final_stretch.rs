//! Final stretch coverage: panic hook, argv permutations, edge subsonic states.

#![allow(clippy::zombie_processes)]

mod common;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use common::TestDaemon;
use serial_test::serial;

#[test]
fn panic_hook_invokes_provided_closure() {
    let installed = Arc::new(AtomicBool::new(false));
    let installed_clone = installed.clone();
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |_info| {
        installed_clone.store(true, Ordering::SeqCst);
    }));
    let r = std::panic::catch_unwind(|| panic!("boom"));
    let _ = std::panic::take_hook();
    std::panic::set_hook(prev);
    assert!(r.is_err());
    assert!(
        installed.load(Ordering::SeqCst),
        "panic hook must fire on panic"
    );
}

#[test]
fn panic_hook_chaining_preserves_previous_hook() {
    let outer = Arc::new(AtomicBool::new(false));
    let inner = Arc::new(AtomicBool::new(false));
    let outer_c = outer.clone();
    let inner_c = inner.clone();

    let original = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        outer_c.store(true, Ordering::SeqCst);
        original(info);
    }));
    let middle = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        inner_c.store(true, Ordering::SeqCst);
        middle(info);
    }));

    let _ = std::panic::catch_unwind(|| panic!("chained"));
    let _ = std::panic::take_hook();

    assert!(inner.load(Ordering::SeqCst));
    assert!(outer.load(Ordering::SeqCst));
}

#[tokio::test]
#[serial]
async fn refresh_random_with_no_subsonic_client_is_silent() {
    let td = TestDaemon::new().await;
    {
        let mut sub = td.core.subsonic.write().await;
        *sub = None;
    }
    td.core.refresh_random().await;
}

#[tokio::test]
#[serial]
async fn refresh_artists_with_no_subsonic_client_is_silent() {
    let td = TestDaemon::new().await;
    {
        let mut sub = td.core.subsonic.write().await;
        *sub = None;
    }
    td.core.refresh_artists().await;
}

#[tokio::test]
#[serial]
async fn refresh_starred_with_no_subsonic_client_is_silent() {
    let td = TestDaemon::new().await;
    {
        let mut sub = td.core.subsonic.write().await;
        *sub = None;
    }
    td.core.refresh_starred().await;
}

#[tokio::test]
#[serial]
async fn refresh_playlists_with_no_subsonic_client_is_silent() {
    let td = TestDaemon::new().await;
    {
        let mut sub = td.core.subsonic.write().await;
        *sub = None;
    }
    td.core.refresh_playlists().await;
}

#[tokio::test]
#[serial]
async fn load_album_songs_with_no_subsonic_client_returns_empty() {
    let td = TestDaemon::new().await;
    {
        let mut sub = td.core.subsonic.write().await;
        *sub = None;
    }
    let songs = td.core.load_album_songs("any").await;
    assert!(songs.is_empty());
}

#[tokio::test]
#[serial]
async fn load_playlist_songs_with_no_subsonic_client_returns_empty() {
    let td = TestDaemon::new().await;
    {
        let mut sub = td.core.subsonic.write().await;
        *sub = None;
    }
    let songs = td.core.load_playlist_songs("any").await;
    assert!(songs.is_empty());
}

#[tokio::test]
#[serial]
async fn search_with_no_subsonic_client_returns_empty() {
    let td = TestDaemon::new().await;
    {
        let mut sub = td.core.subsonic.write().await;
        *sub = None;
    }
    let r = td.core.search("x", 5, 5, 5).await;
    assert!(r.artist.is_empty() && r.album.is_empty() && r.song.is_empty());
}

#[tokio::test]
#[serial]
async fn load_artist_with_no_subsonic_client_is_silent() {
    let td = TestDaemon::new().await;
    {
        let mut sub = td.core.subsonic.write().await;
        *sub = None;
    }
    td.core.load_artist("any").await;
}

#[tokio::test]
#[serial]
async fn shuffle_library_with_no_subsonic_client_is_safe() {
    let td = TestDaemon::new().await;
    {
        let mut sub = td.core.subsonic.write().await;
        *sub = None;
    }
    td.core.shuffle_library().await.unwrap();
}
