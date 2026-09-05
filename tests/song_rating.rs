//! `set_song_rating` propagates the new rating to the target song across
//! every cached list (queue, random, starred, album cache, playlist cache,
//! now-playing) and rolls back the optimistic update if the server RPC fails.

mod common;

use common::{song, TestDaemon};
use serial_test::serial;

#[tokio::test]
#[serial]
async fn rating_marks_the_target_song_across_all_cached_lists() {
    let td = TestDaemon::new().await;
    td.fake_subsonic.expect_set_rating().await;
    {
        let mut s = td.state.write().await;
        s.queue = vec![song("s1", "Target"), song("s2", "Other")];
        s.library.random_songs = vec![song("s1", "Target"), song("s1", "Duplicate")];
        s.library.random_album_songs = vec![song("s1", "Album")];
        s.library.starred_songs = vec![song("s1", "Target")];
        s.library
            .album_songs_cache
            .insert("alb".into(), vec![song("s1", "Target")]);
        s.library
            .playlist_songs_cache
            .insert("pl".into(), vec![song("s1", "Target")]);
        s.now_playing.song = Some(song("s1", "Target"));
    }

    let new_rating = td.core.set_song_rating("s1", 4).await.unwrap();
    assert_eq!(new_rating, Some(4));

    let s = td.state.read().await;
    assert_eq!(s.library.random_songs[1].user_rating, Some(4));
    assert_eq!(s.library.random_album_songs[0].user_rating, Some(4));
    assert_eq!(s.queue[0].user_rating, Some(4), "target marked in queue");
    assert_eq!(
        s.queue[1].user_rating, None,
        "a different queued song is not marked"
    );
    assert_eq!(
        s.library.random_songs[0].user_rating,
        Some(4),
        "target marked in random"
    );
    assert_eq!(
        s.library.starred_songs[0].user_rating,
        Some(4),
        "target marked in starred"
    );
    assert_eq!(
        s.library.album_songs_cache["alb"][0].user_rating,
        Some(4),
        "target marked in the album cache"
    );
    assert_eq!(
        s.library.playlist_songs_cache["pl"][0].user_rating,
        Some(4),
        "target marked in the playlist cache"
    );
    assert_eq!(
        s.now_playing.song.as_ref().unwrap().user_rating,
        Some(4),
        "target marked in now-playing"
    );
}

#[tokio::test]
#[serial]
async fn rating_zero_clears_the_rating() {
    let td = TestDaemon::new().await;
    td.fake_subsonic.expect_set_rating().await;
    {
        let mut s = td.state.write().await;
        let mut rated = song("s1", "Target");
        rated.user_rating = Some(3);
        s.queue = vec![rated];
    }

    let new_rating = td.core.set_song_rating("s1", 0).await.unwrap();
    assert_eq!(new_rating, None);

    let s = td.state.read().await;
    assert_eq!(s.queue[0].user_rating, None, "rating cleared in queue");
}

#[tokio::test]
#[serial]
async fn rating_clamps_above_five() {
    let td = TestDaemon::new().await;
    td.fake_subsonic.expect_set_rating().await;
    {
        let mut s = td.state.write().await;
        s.queue = vec![song("s1", "Target")];
    }

    let new_rating = td.core.set_song_rating("s1", 9).await.unwrap();
    assert_eq!(new_rating, Some(5), "rating clamps to 5");
}

#[tokio::test]
#[serial]
async fn rating_rolls_back_on_rpc_failure() {
    // No expect_set_rating() mounted: the server call 404s, so the optimistic
    // cache update must be rolled back to the old rating.
    let td = TestDaemon::new().await;
    {
        let mut s = td.state.write().await;
        let mut rated = song("s1", "Target");
        rated.user_rating = Some(2);
        s.queue = vec![rated];
    }

    let result = td.core.set_song_rating("s1", 5).await;
    assert!(result.is_err(), "RPC failure must surface as an error");

    let s = td.state.read().await;
    assert_eq!(
        s.queue[0].user_rating,
        Some(2),
        "rating rolls back to the pre-toggle value on RPC failure"
    );
}

async fn wait_for_rating_request(td: &TestDaemon) {
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            if td
                .fake_subsonic
                .received_requests()
                .await
                .iter()
                .any(|r| r.url.path() == "/rest/setRating")
            {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
#[serial]
async fn failed_earlier_rating_cannot_roll_back_a_later_success() {
    let td = TestDaemon::new().await;
    td.fake_subsonic.expect_rating_response(4, 500, 300).await;
    td.fake_subsonic.expect_rating_response(5, 200, 0).await;
    let mut rated = song("s1", "Target");
    rated.user_rating = Some(2);
    td.state.write().await.queue = vec![rated];
    let core = td.core.clone();
    let first = tokio::spawn(async move { core.set_song_rating("s1", 4).await });
    wait_for_rating_request(&td).await;
    let second = td.core.set_song_rating("s1", 5).await;
    assert!(first.await.unwrap().is_err());
    assert_eq!(second.unwrap(), Some(5));
    assert_eq!(td.state.read().await.queue[0].user_rating, Some(5));
}

#[tokio::test]
#[serial]
async fn concurrent_rating_events_follow_server_write_order() {
    use ferrosonic::ipc::DaemonEvent;
    let td = TestDaemon::new().await;
    td.fake_subsonic.expect_rating_response(4, 200, 300).await;
    td.fake_subsonic.expect_rating_response(5, 200, 0).await;
    td.state.write().await.queue = vec![song("s1", "Target")];
    let mut events = td.core.event_tx.subscribe();
    let core = td.core.clone();
    let first = tokio::spawn(async move { core.set_song_rating("s1", 4).await });
    wait_for_rating_request(&td).await;
    td.core.set_song_rating("s1", 5).await.unwrap();
    first.await.unwrap().unwrap();
    let mut ratings = Vec::new();
    while let Ok(event) = events.try_recv() {
        if let DaemonEvent::SongRatingChanged { rating, .. } = event {
            ratings.push(rating);
        }
    }
    assert_eq!(ratings, vec![Some(4), Some(5)]);
}

#[tokio::test]
#[serial]
async fn old_server_rating_response_cannot_mutate_new_server_caches() {
    for status in [200, 500] {
        let td = TestDaemon::new().await;
        td.fake_subsonic
            .expect_rating_response(4, status, 300)
            .await;
        td.state.write().await.queue = vec![song("s1", "Old server")];
        let mut events = td.core.event_tx.subscribe();
        let core = td.core.clone();
        let pending = tokio::spawn(async move { core.set_song_rating("s1", 4).await });
        wait_for_rating_request(&td).await;
        {
            let mut state = td.state.write().await;
            td.core.bump_config_gen_for_test();
            let mut current = song("s1", "New server");
            current.user_rating = Some(3);
            state.queue = vec![current];
        }
        let result = pending.await.unwrap();
        assert_eq!(result.is_ok(), status == 200);
        assert_eq!(td.state.read().await.queue[0].user_rating, Some(3));
        assert!(
            events.try_recv().is_err(),
            "old-server ratings must not be broadcast"
        );
    }
}
