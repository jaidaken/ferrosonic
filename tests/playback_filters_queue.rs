//! Playback filters actually excluding songs at the 3 real queue-entry
//! points: `EnqueueSongs` (all 3 modes), `shuffle_library`, and
//! auto-continue. `passes_filters` itself is exhaustively unit-tested in
//! `src/daemon/playback_filters.rs`; this file proves the wiring.

mod common;

use std::time::Duration;

use common::{song, TestDaemon};
use ferrosonic::config::PlaybackFilters;
use ferrosonic::ipc::client::{DaemonClient, InProcessClient};
use ferrosonic::ipc::protocol::{DaemonEvent, DaemonRequest, EnqueueMode};
use serial_test::serial;
use tokio::sync::broadcast::Receiver;

async fn recv_matching<F>(rx: &mut Receiver<DaemonEvent>, pred: F) -> bool
where
    F: Fn(&DaemonEvent) -> bool,
{
    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    loop {
        match tokio::time::timeout_at(deadline, rx.recv()).await {
            Ok(Ok(ev)) => {
                if pred(&ev) {
                    return true;
                }
            }
            _ => return false,
        }
    }
}

fn rated(id: &str, rating: u8) -> ferrosonic::subsonic::models::Child {
    let mut s = song(id, id);
    s.user_rating = Some(rating);
    s
}

#[tokio::test]
#[serial]
async fn enqueue_replace_filters_out_low_rated_songs() {
    let td = TestDaemon::new().await;
    td.state.write().await.config.playback_filters = PlaybackFilters {
        min_rating: 2,
        ..Default::default()
    };
    let client = InProcessClient::new(td.core.clone());

    client
        .request(DaemonRequest::EnqueueSongs {
            songs: vec![rated("a", 1), song("b", "b"), rated("c", 3)],
            mode: EnqueueMode::Replace { play_from: None },
        })
        .await
        .unwrap();

    let s = td.state.read().await;
    assert_eq!(
        s.queue.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(),
        vec!["b", "c"],
        "the 1-star song must be excluded, unrated and 3-star kept"
    );
}

#[tokio::test]
#[serial]
async fn enqueue_replace_remaps_play_from_to_the_surviving_target_song() {
    let td = TestDaemon::new().await;
    td.state.write().await.config.playback_filters = PlaybackFilters {
        min_rating: 2,
        ..Default::default()
    };
    td.fake_subsonic.expect_ping().await;
    let client = InProcessClient::new(td.core.clone());

    // play_from=2 targets "c" (index 2); "a" (index 0) is filtered out first,
    // so "c" must land at index 1 in the filtered queue and still be the
    // play target, not silently shifted to the wrong song.
    client
        .request(DaemonRequest::EnqueueSongs {
            songs: vec![rated("a", 1), song("b", "b"), rated("c", 5)],
            mode: EnqueueMode::Replace { play_from: Some(2) },
        })
        .await
        .unwrap();

    let s = td.state.read().await;
    assert_eq!(s.queue_position, Some(1), "c survives at filtered index 1");
    assert_eq!(s.queue[1].id, "c");
}

#[tokio::test]
#[serial]
async fn enqueue_replace_falls_back_to_first_survivor_when_target_song_is_filtered_out() {
    let td = TestDaemon::new().await;
    td.state.write().await.config.playback_filters = PlaybackFilters {
        min_rating: 2,
        ..Default::default()
    };
    td.fake_subsonic.expect_ping().await;
    let client = InProcessClient::new(td.core.clone());

    // play_from=0 targets "a", which is itself filtered out by min_rating.
    client
        .request(DaemonRequest::EnqueueSongs {
            songs: vec![rated("a", 1), song("b", "b")],
            mode: EnqueueMode::Replace { play_from: Some(0) },
        })
        .await
        .unwrap();

    let s = td.state.read().await;
    assert_eq!(
        s.queue_position,
        Some(0),
        "falls back to the first surviving song rather than leaving the queue stopped"
    );
    assert_eq!(s.queue[0].id, "b");
}

#[tokio::test]
#[serial]
async fn enqueue_replace_with_everything_filtered_emits_notification_and_no_op() {
    let td = TestDaemon::new().await;
    td.state.write().await.config.playback_filters = PlaybackFilters {
        min_rating: 5,
        ..Default::default()
    };
    {
        let mut s = td.state.write().await;
        s.queue = vec![song("existing", "Existing")];
    }
    let mut rx = td.core.subscribe();
    let client = InProcessClient::new(td.core.clone());

    client
        .request(DaemonRequest::EnqueueSongs {
            songs: vec![rated("a", 1), rated("b", 2)],
            mode: EnqueueMode::Replace { play_from: Some(0) },
        })
        .await
        .unwrap();

    assert!(
        recv_matching(&mut rx, |e| matches!(
            e,
            DaemonEvent::Notification { is_error: true, .. }
        ))
        .await,
        "excluding every candidate song must notify the user"
    );
    let s = td.state.read().await;
    assert_eq!(
        s.queue.len(),
        1,
        "the existing queue must be untouched when everything offered was filtered out"
    );
    assert_eq!(s.queue[0].id, "existing");
}

#[tokio::test]
#[serial]
async fn enqueue_append_filters_out_low_rated_songs() {
    let td = TestDaemon::new().await;
    td.state.write().await.config.playback_filters = PlaybackFilters {
        min_rating: 2,
        ..Default::default()
    };
    {
        let mut s = td.state.write().await;
        s.queue = vec![song("existing", "Existing")];
    }
    let client = InProcessClient::new(td.core.clone());

    client
        .request(DaemonRequest::EnqueueSongs {
            songs: vec![rated("a", 1), rated("c", 4)],
            mode: EnqueueMode::Append,
        })
        .await
        .unwrap();

    let s = td.state.read().await;
    assert_eq!(
        s.queue.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(),
        vec!["existing", "c"]
    );
}

#[tokio::test]
#[serial]
async fn enqueue_insert_after_filters_out_low_rated_songs() {
    let td = TestDaemon::new().await;
    td.state.write().await.config.playback_filters = PlaybackFilters {
        min_rating: 2,
        ..Default::default()
    };
    {
        let mut s = td.state.write().await;
        s.queue = vec![song("a", "A"), song("z", "Z")];
    }
    let client = InProcessClient::new(td.core.clone());

    client
        .request(DaemonRequest::EnqueueSongs {
            songs: vec![rated("x", 1), rated("y", 3)],
            mode: EnqueueMode::InsertAfter(0),
        })
        .await
        .unwrap();

    let s = td.state.read().await;
    assert_eq!(
        s.queue.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(),
        vec!["a", "y", "z"],
        "the 1-star song must not be inserted"
    );
}

#[tokio::test]
#[serial]
async fn shuffle_library_filters_the_fetched_random_batch() {
    let td = TestDaemon::new().await;
    td.state.write().await.config.playback_filters = PlaybackFilters {
        min_rating: 2,
        ..Default::default()
    };
    td.fake_subsonic
        .expect_random_songs_rated(&[("Low", 1), ("Mid", 3), ("High", 5)])
        .await;

    td.core.shuffle_library().await.unwrap();

    let s = td.state.read().await;
    assert_eq!(
        s.queue.len(),
        2,
        "the 1-star song must be excluded from the shuffled queue"
    );
    assert!(!s.queue.iter().any(|c| c.title == "Low"));
}

#[tokio::test]
#[serial]
async fn auto_continue_filters_fetched_random_candidates() {
    let td = TestDaemon::new().await;
    {
        let mut s = td.state.write().await;
        s.config.playback_filters = PlaybackFilters {
            min_rating: 2,
            ..Default::default()
        };
        s.config.auto_continue = true;
        s.config.repeat_mode = ferrosonic::config::RepeatMode::Off;
        s.queue = vec![song("s1", "A")];
        s.queue_position = Some(0);
    }
    td.fake_subsonic
        .expect_random_songs_rated(&[("Low", 1), ("High", 5)])
        .await;

    td.core.next_track().await.unwrap();

    let s = td.state.read().await;
    assert!(
        s.queue.iter().any(|c| c.title == "High"),
        "the passing candidate must be appended"
    );
    assert!(
        !s.queue.iter().any(|c| c.title == "Low"),
        "the 1-star candidate must be excluded from auto-continue"
    );
}
