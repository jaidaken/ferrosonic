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

/// End-to-end: rating a song the user highlighted in the Library must reach
/// the *client-side* list the Library song pane renders from
/// (`client.artists.songs`), not only the daemon's caches.
///
/// The Queue page renders from the shared daemon state, so in standalone
/// mode it updates whether or not the broadcast event is delivered. The
/// Library pane has no such shortcut: it only changes if
/// `SongRatingChanged` actually reaches the event pump. A regression here
/// would look exactly like "ratings work in the Queue but not the Library".
#[tokio::test]
#[serial]
async fn rating_reaches_the_client_library_song_pane_over_the_event_bus() {
    use ferrosonic::app::event_pump::apply_event;
    use ferrosonic::ipc::protocol::DaemonEvent;

    let td = TestDaemon::new().await;
    td.fake_subsonic.expect_set_rating().await;

    let config = ferrosonic::config::Config::new();
    let client_state = ferrosonic::app::state::new_shared_client_state(&config);
    client_state.write().await.artists.songs = vec![song("s1", "Target"), song("s2", "Other")];

    let mut rx = td.core.subscribe();
    td.core.set_song_rating("s1", 4).await.unwrap();

    let ev = tokio::time::timeout(std::time::Duration::from_secs(5), rx.recv())
        .await
        .expect("a rating event must be broadcast")
        .expect("broadcast channel stays open");
    assert!(
        matches!(&ev, DaemonEvent::SongRatingChanged { id, rating } if id == "s1" && *rating == Some(4)),
        "expected SongRatingChanged, got {ev:?}"
    );

    let cover_art = std::sync::Arc::new(std::sync::Mutex::new(
        ferrosonic::ui::cover_art::CoverArtState {
            picker: None,
            protocol_type: None,
            cell_size: (8, 16),
            current_id: None,
            image: None,
            protocol: None,
            chafa_cache: None,
        },
    ));
    let client: std::sync::Arc<dyn ferrosonic::ipc::client::DaemonClient> = std::sync::Arc::new(
        ferrosonic::ipc::client::InProcessClient::new(td.core.clone()),
    );
    apply_event(&td.state, &client_state, &client, &cover_art, ev).await;

    let cs = client_state.read().await;
    assert_eq!(
        cs.artists.songs[0].user_rating,
        Some(4),
        "the highlighted Library row must show the new rating"
    );
    assert_eq!(cs.artists.songs[1].user_rating, None);
}

/// A rating change must reach the persisted queue, or it silently reverts
/// on the next start.
///
/// `queue.json` is only rewritten when something pokes the save channel,
/// which until now only `emit_queue` did. Rating mutates `daemon.queue`
/// in memory and emits `SongRatingChanged` instead, so the rating shown
/// after a restart was whatever happened to be current at the last queue
/// mutation -- a track rated 1 and later 5 came back as 1.
#[tokio::test]
#[serial]
async fn a_rating_change_is_persisted_to_the_queue_snapshot() {
    use ferrosonic::daemon::persistence::QueueSnapshot;

    let dir = common::tempdir();
    std::env::set_var("FERROSONIC_CONFIG_DIR", dir.path());

    let td = TestDaemon::new().await;
    td.fake_subsonic.expect_set_rating().await;
    td.state.write().await.queue = vec![song("s1", "Target")];

    // An earlier rating, persisted the way any queue mutation would be.
    td.core.set_song_rating("s1", 1).await.unwrap();
    // The rating the user actually wants to keep.
    td.core.set_song_rating("s1", 5).await.unwrap();

    // The persistence task is debounced; give it room to flush.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    let persisted = loop {
        if let Some(snap) = QueueSnapshot::load() {
            if snap.queue.first().and_then(|s| s.user_rating) == Some(5) {
                break Some(snap);
            }
        }
        if std::time::Instant::now() > deadline {
            break QueueSnapshot::load();
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    };

    let snap = persisted.expect("queue.json must be written after a rating change");
    assert_eq!(
        snap.queue[0].user_rating,
        Some(5),
        "the persisted queue must carry the latest rating, not a stale one"
    );
}

/// Stars share the in-place queue edit and the same staleness.
#[tokio::test]
#[serial]
async fn a_star_change_is_persisted_to_the_queue_snapshot() {
    use ferrosonic::daemon::persistence::QueueSnapshot;

    let dir = common::tempdir();
    std::env::set_var("FERROSONIC_CONFIG_DIR", dir.path());

    let td = TestDaemon::new().await;
    td.fake_subsonic.expect_star().await;
    td.state.write().await.queue = vec![song("s1", "Target")];

    td.core.toggle_star_song("s1").await.unwrap();

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        if let Some(snap) = QueueSnapshot::load() {
            if snap.queue.first().is_some_and(|s| s.starred.is_some()) {
                return;
            }
        }
        assert!(
            std::time::Instant::now() <= deadline,
            "the persisted queue must carry the new star"
        );
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
}
