//! Library refresh flows: starred, random, artists, playlists.

mod common;

use common::{song, TestDaemon};
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

// Closes the config_gen_changed seam: a refresh whose server changed mid-request
// (config_gen bumped after its snapshot) must discard the now-stale result.
#[tokio::test]
#[serial]
async fn refresh_starred_discards_result_when_config_changed_mid_request() {
    let td = TestDaemon::new().await;
    // Hold the response so we can bump config_gen while the refresh is in flight.
    td.fake_subsonic
        .expect_starred_with_delay(&["stale-server"], 300)
        .await;
    {
        let mut s = td.state.write().await;
        s.library.starred_songs = vec![song("orig", "Original")];
    }

    let core = td.core.clone();
    let handle = tokio::spawn(async move { core.refresh_starred().await });
    // Let the refresh capture its config_gen snapshot and park on the delayed
    // fetch, then simulate a server change (config_gen bump) mid-request.
    for _ in 0..20 {
        tokio::task::yield_now().await;
    }
    td.core.bump_config_gen_for_test();
    handle.await.expect("refresh task panicked");

    let s = td.state.read().await;
    assert_eq!(
        s.library.starred_songs.len(),
        1,
        "stale starred result must be discarded after a mid-request config change"
    );
    assert_eq!(
        s.library.starred_songs[0].id, "orig",
        "the pre-existing list must survive; the stale-server result must not land"
    );
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
