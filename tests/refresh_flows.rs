//! Library refresh flows: starred, random, artists, playlists.

mod common;

use common::TestDaemon;
use serial_test::serial;

#[tokio::test]
#[serial]
async fn refresh_starred_populates_library() {
    let td = TestDaemon::new().await;
    td.fake_subsonic
        .expect_starred_with(&["Track A", "Track B"])
        .await;

    td.core.refresh_starred().await;

    let s = td.state.read().await;
    assert_eq!(s.library.starred_songs.len(), 2);
    assert_eq!(s.library.starred_songs[0].title, "Track A");
}

#[tokio::test]
#[serial]
async fn refresh_random_populates_library() {
    let td = TestDaemon::new().await;
    td.fake_subsonic
        .expect_random_songs(&["One", "Two", "Three"])
        .await;

    td.core.refresh_random().await;

    let s = td.state.read().await;
    assert_eq!(s.library.random_songs.len(), 3);
    assert_eq!(s.library.random_songs[1].title, "Two");
}

#[tokio::test]
#[serial]
async fn refresh_artists_populates_library() {
    let td = TestDaemon::new().await;
    td.fake_subsonic
        .expect_artists(&["The Cure", "Joy Division"])
        .await;

    td.core.refresh_artists().await;

    let s = td.state.read().await;
    assert_eq!(s.library.artists.len(), 2);
    let names: Vec<&str> = s.library.artists.iter().map(|a| a.name.as_str()).collect();
    assert!(names.contains(&"The Cure"));
    assert!(names.contains(&"Joy Division"));
}

#[tokio::test]
#[serial]
async fn refresh_playlists_populates_library() {
    let td = TestDaemon::new().await;
    td.fake_subsonic.expect_playlists().await;

    td.core.refresh_playlists().await;

    let s = td.state.read().await;
    assert_eq!(s.library.playlists.len(), 0, "fake returns empty list");
}

#[tokio::test]
#[serial]
async fn refresh_without_subsonic_client_is_safe() {
    let td = TestDaemon::new().await;
    {
        let mut sub = td.core.subsonic.write().await;
        *sub = None;
    }

    td.core.refresh_starred().await;
    td.core.refresh_random().await;
    td.core.refresh_artists().await;
    td.core.refresh_playlists().await;

    let s = td.state.read().await;
    assert!(s.library.starred_songs.is_empty());
    assert!(s.library.random_songs.is_empty());
    assert!(s.library.artists.is_empty());
    assert!(s.library.playlists.is_empty());
}
