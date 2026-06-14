//! Star toggle propagates the marker to the right song across every cached
//! list (queue, random, album cache, playlist cache, now-playing) and updates
//! starred_ids. Kills the apply_star_to_cached / sync_starred_songs body and
//! `song.id == song_id` -> `!=` mutants, which the existing star tests (asserting
//! only the RPC path) leave alive.

mod common;

use common::{song, TestDaemon};
use serial_test::serial;

#[tokio::test]
#[serial]
async fn star_marks_the_target_song_across_all_cached_lists() {
    let td = TestDaemon::new().await;
    td.fake_subsonic.expect_star().await;
    td.fake_subsonic.expect_starred().await;
    {
        let mut s = td.state.write().await;
        s.queue = vec![song("s1", "Target"), song("s2", "Other")];
        s.library.random_songs = vec![song("s1", "Target")];
        s.library
            .album_songs_cache
            .insert("alb".into(), vec![song("s1", "Target")]);
        s.library
            .playlist_songs_cache
            .insert("pl".into(), vec![song("s1", "Target")]);
        s.now_playing.song = Some(song("s1", "Target"));
    }

    let starred = td.core.toggle_star_song("s1").await.unwrap();
    assert!(starred, "toggling an unstarred song reports starred=true");

    let s = td.state.read().await;
    assert!(s.queue[0].starred.is_some(), "target marked in queue");
    assert!(s.queue[1].starred.is_none(), "a different queued song is not marked");
    assert!(s.library.random_songs[0].starred.is_some(), "target marked in random");
    assert!(
        s.library.album_songs_cache["alb"][0].starred.is_some(),
        "target marked in the album cache"
    );
    assert!(
        s.library.playlist_songs_cache["pl"][0].starred.is_some(),
        "target marked in the playlist cache"
    );
    assert!(
        s.now_playing.song.as_ref().unwrap().starred.is_some(),
        "target marked in now-playing"
    );
}

#[tokio::test]
#[serial]
async fn star_updates_starred_index_when_refresh_unavailable() {
    // No expect_starred(): the post-toggle server refresh fails, so the
    // optimistic sync_starred_songs result persists and is observable.
    let td = TestDaemon::new().await;
    td.fake_subsonic.expect_star().await;
    {
        let mut s = td.state.write().await;
        s.queue = vec![song("s1", "Target")];
    }

    td.core.toggle_star_song("s1").await.unwrap();

    let s = td.state.read().await;
    assert!(s.library.starred_ids.contains("s1"), "starred_ids gains the song");
    assert!(
        s.library.starred_songs.iter().any(|x| x.id == "s1"),
        "starred_songs gains the song from a cached source"
    );
}

#[tokio::test]
#[serial]
async fn star_appends_to_a_nonempty_starred_list_when_refresh_unavailable() {
    // A pre-existing entry for a different song makes the `already` check
    // meaningful: `==` -> `!=` would treat s1 as present and skip the append.
    let td = TestDaemon::new().await;
    td.fake_subsonic.expect_star().await;
    {
        let mut s = td.state.write().await;
        s.queue = vec![song("s1", "Target")];
        let mut other = song("s2", "Other");
        other.starred = Some("x".into());
        s.library.starred_songs = vec![other];
    }

    td.core.toggle_star_song("s1").await.unwrap();

    let s = td.state.read().await;
    assert!(
        s.library.starred_songs.iter().any(|x| x.id == "s1"),
        "s1 is appended to the existing starred list"
    );
    assert!(
        s.library.starred_songs.iter().any(|x| x.id == "s2"),
        "the existing starred entry is retained"
    );
}

#[tokio::test]
#[serial]
async fn unstar_clears_the_marker_across_cached_lists() {
    let td = TestDaemon::new().await;
    td.fake_subsonic.expect_unstar().await;
    td.fake_subsonic.expect_starred().await;
    {
        let mut s = td.state.write().await;
        let mut starred = song("s1", "Target");
        starred.starred = Some("2026-01-01T00:00:00Z".into());
        s.queue = vec![starred.clone()];
        s.now_playing.song = Some(starred);
        s.library.starred_ids.insert("s1".into());
    }

    let now_starred = td.core.toggle_star_song("s1").await.unwrap();
    assert!(!now_starred, "toggling a starred song reports starred=false");

    let s = td.state.read().await;
    assert!(s.queue[0].starred.is_none(), "marker cleared in queue");
    assert!(
        s.now_playing.song.as_ref().unwrap().starred.is_none(),
        "marker cleared in now-playing"
    );
    assert!(!s.library.starred_ids.contains("s1"), "starred_ids loses the song");
}
