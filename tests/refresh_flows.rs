//! Library refresh flows: starred, random, artists, playlists.

mod common;

use common::{song, TestDaemon};
use ferrosonic::ipc::protocol::QuickPlayAlbumKind;
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
async fn refresh_random_album_populates_library() {
    let td = TestDaemon::new().await;
    td.fake_subsonic
        .expect_random_album("alb-1", "Test Album", &["One", "Two"])
        .await;

    td.core.refresh_random_album().await;

    let s = td.state.read().await;
    assert_eq!(s.library.random_album_songs.len(), 2);
    assert_eq!(s.library.random_album_songs[0].title, "One");
}

#[tokio::test]
#[serial]
async fn refresh_random_album_with_no_albums_clears_and_notifies_nothing_bad() {
    let td = TestDaemon::new().await;
    td.fake_subsonic.expect_no_random_album().await;
    {
        let mut s = td.state.write().await;
        s.library.random_album_songs = vec![song("stale", "Stale")];
    }

    td.core.refresh_random_album().await;

    let s = td.state.read().await;
    assert!(
        s.library.random_album_songs.is_empty(),
        "an empty library must clear any previously cached random album"
    );
}

#[tokio::test]
#[serial]
async fn refresh_quick_play_album_populates_each_category_independently() {
    let td = TestDaemon::new().await;
    for (kind, query, id, title) in [
        (QuickPlayAlbumKind::Newest, "newest", "new", "New Track"),
        (
            QuickPlayAlbumKind::Recent,
            "recent",
            "recent",
            "Recent Track",
        ),
        (
            QuickPlayAlbumKind::Frequent,
            "frequent",
            "often",
            "Frequent Track",
        ),
        (QuickPlayAlbumKind::Highest, "highest", "top", "Top Track"),
    ] {
        td.fake_subsonic
            .expect_quick_play_album(query, id, "Selected Album", &[title])
            .await;
        td.core.refresh_quick_play_album(kind).await;
    }

    let state = td.state.read().await;
    assert_eq!(state.library.quick_play_album_songs.len(), 4);
    assert_eq!(
        state.library.quick_play_album_songs[&QuickPlayAlbumKind::Newest][0].title,
        "New Track"
    );
    assert_eq!(
        state.library.quick_play_album_songs[&QuickPlayAlbumKind::Highest][0].title,
        "Top Track"
    );
}

#[tokio::test]
#[serial]
async fn empty_quick_play_category_clears_only_that_category() {
    let td = TestDaemon::new().await;
    td.fake_subsonic.expect_no_quick_play_album("recent").await;
    {
        let mut state = td.state.write().await;
        state
            .library
            .quick_play_album_songs
            .insert(QuickPlayAlbumKind::Recent, vec![song("stale", "Stale")]);
        state
            .library
            .quick_play_album_songs
            .insert(QuickPlayAlbumKind::Newest, vec![song("keep", "Keep")]);
    }

    td.core
        .refresh_quick_play_album(QuickPlayAlbumKind::Recent)
        .await;

    let state = td.state.read().await;
    assert!(!state
        .library
        .quick_play_album_songs
        .contains_key(&QuickPlayAlbumKind::Recent));
    assert!(state
        .library
        .quick_play_album_songs
        .contains_key(&QuickPlayAlbumKind::Newest));
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
    td.core.refresh_random_album().await;
    td.core.refresh_artists().await;
    td.core.refresh_playlists().await;

    let s = td.state.read().await;
    assert!(s.library.starred_songs.is_empty());
    assert!(s.library.random_songs.is_empty());
    assert!(s.library.random_album_songs.is_empty());
    assert!(s.library.artists.is_empty());
    assert!(s.library.playlists.is_empty());
}

#[tokio::test]
#[serial]
async fn stale_empty_random_album_does_not_clear_current_library() {
    let td = TestDaemon::new().await;
    td.fake_subsonic
        .expect_no_random_album_with_delay(500)
        .await;
    td.state.write().await.library.random_album_songs = vec![song("current", "Current")];
    let core = td.core.clone();
    let task = tokio::spawn(async move { core.refresh_random_album().await });
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            if td
                .fake_subsonic
                .received_requests()
                .await
                .iter()
                .any(|r| r.url.path() == "/rest/getAlbumList2")
            {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    td.core.bump_config_gen_for_test();
    task.await.unwrap();
    assert_eq!(
        td.state.read().await.library.random_album_songs[0].id,
        "current"
    );
}

#[tokio::test]
#[serial]
async fn stale_quick_play_reply_does_not_clear_current_category() {
    let td = TestDaemon::new().await;
    td.fake_subsonic
        .expect_no_quick_play_album_with_delay("newest", 500)
        .await;
    td.state
        .write()
        .await
        .library
        .quick_play_album_songs
        .insert(QuickPlayAlbumKind::Newest, vec![song("current", "Current")]);
    let core = td.core.clone();
    let task = tokio::spawn(async move {
        core.refresh_quick_play_album(QuickPlayAlbumKind::Newest)
            .await;
    });
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            if td
                .fake_subsonic
                .received_requests()
                .await
                .iter()
                .any(|request| request.url.path() == "/rest/getAlbumList2")
            {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    td.core.bump_config_gen_for_test();
    task.await.unwrap();
    assert_eq!(
        td.state.read().await.library.quick_play_album_songs[&QuickPlayAlbumKind::Newest][0].id,
        "current"
    );
}
