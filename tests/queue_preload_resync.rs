//! Regression for #3: a queue mutation while playing must re-align mpv's
//! gapless preload (slot 1) with the queue's new next track, otherwise the
//! old preloaded audio plays while the metadata shows the new track.

mod common;

use common::{song, songs, TestDaemon};
use ferrosonic::daemon::state::PlaybackState;
use ferrosonic::ipc::client::{DaemonClient, InProcessClient};
use ferrosonic::ipc::protocol::{DaemonRequest, EnqueueMode};
use ferrosonic::subsonic::models::Child;
use serde_json::Value;
use serial_test::serial;

async fn set_playing(td: &TestDaemon, queue: Vec<Child>, pos: usize) {
    let mut s = td.state.write().await;
    s.queue = queue;
    s.queue_position = Some(pos);
    s.now_playing.state = PlaybackState::Playing;
}

/// Whether mpv was told to preload (append) a track whose URL carries `id`.
async fn preloaded(td: &TestDaemon, id: &str) -> bool {
    td.fake_mpv
        .wait_for(2000, |cmds| {
            cmds.iter().any(|c| {
                c.first().and_then(Value::as_str) == Some("loadfile")
                    && c.get(2).and_then(Value::as_str) == Some("append")
                    && c.get(1)
                        .and_then(Value::as_str)
                        .is_some_and(|u| u.contains(id))
            })
        })
        .await
}

#[tokio::test]
#[serial]
async fn insert_after_current_repreloads_the_new_next() {
    let td = TestDaemon::new().await;
    let client = InProcessClient::new(td.core.clone());
    set_playing(&td, songs("t", 2), 0).await;

    client
        .request(DaemonRequest::EnqueueSongs {
            songs: vec![song("xins", "Inserted")],
            mode: EnqueueMode::InsertAfter(0),
        })
        .await
        .unwrap();

    assert!(
        preloaded(&td, "xins").await,
        "InsertAfter into the next slot must re-preload the inserted track; commands: {:?}",
        td.fake_mpv.commands().await
    );
}

#[tokio::test]
#[serial]
async fn resync_drops_the_stale_preload_before_re_preloading() {
    let td = TestDaemon::new().await;
    let client = InProcessClient::new(td.core.clone());
    set_playing(&td, songs("t", 3), 0).await;
    // Seed an existing gapless preload: mpv playlist = [current, old-next].
    td.fake_mpv
        .set_playlist(vec!["current".into(), "old-next".into()])
        .await;

    client
        .request(DaemonRequest::EnqueueSongs {
            songs: vec![song("xins", "Inserted")],
            mode: EnqueueMode::InsertAfter(0),
        })
        .await
        .unwrap();

    let dropped_stale = td
        .fake_mpv
        .wait_for(2000, |cmds| {
            cmds.iter().any(|c| {
                c.first().and_then(Value::as_str) == Some("playlist-remove")
                    && c.get(1).and_then(Value::as_u64) == Some(1)
            })
        })
        .await;
    assert!(
        dropped_stale,
        "resync must drop the stale slot-1 preload before re-preloading; commands: {:?}",
        td.fake_mpv.commands().await
    );
}

#[tokio::test]
#[serial]
async fn insert_before_current_keeps_now_playing_pointer() {
    let td = TestDaemon::new().await;
    let client = InProcessClient::new(td.core.clone());
    set_playing(&td, songs("t", 4), 2).await; // playing t-2

    client
        .request(DaemonRequest::EnqueueSongs {
            songs: vec![song("xins", "Inserted")],
            mode: EnqueueMode::InsertAfter(0), // inserts at index 1, before current
        })
        .await
        .unwrap();

    let s = td.state.read().await;
    assert_eq!(
        s.queue_position,
        Some(3),
        "position must follow the playing song past an earlier insert"
    );
    assert_eq!(
        s.queue[3].id, "t-2",
        "queue_position must still point at the playing track"
    );
}

#[tokio::test]
#[serial]
async fn resync_does_not_remove_when_nothing_is_preloaded() {
    let td = TestDaemon::new().await;
    // Playing the only track: mpv playlist holds just the current entry, so
    // there is no slot-1 preload to drop.
    set_playing(&td, songs("t", 1), 0).await;
    td.fake_mpv.set_playlist(vec!["current".into()]).await;

    td.core.shuffle_queue().await; // resync runs synchronously here

    let removed_slot1 = td.fake_mpv.commands().await.iter().any(|c| {
        c.first().and_then(Value::as_str) == Some("playlist-remove")
            && c.get(1).and_then(Value::as_u64) == Some(1)
    });
    assert!(
        !removed_slot1,
        "resync must not remove slot 1 when only the current track is loaded; commands: {:?}",
        td.fake_mpv.commands().await
    );
}

#[tokio::test]
#[serial]
async fn append_when_current_is_last_preloads_appended() {
    let td = TestDaemon::new().await;
    let client = InProcessClient::new(td.core.clone());
    set_playing(&td, vec![song("only", "Only")], 0).await; // current is last

    client
        .request(DaemonRequest::EnqueueSongs {
            songs: vec![song("app", "Appended")],
            mode: EnqueueMode::Append,
        })
        .await
        .unwrap();

    assert!(
        preloaded(&td, "app").await,
        "Append when current is last must preload the appended track; commands: {:?}",
        td.fake_mpv.commands().await
    );
}

#[tokio::test]
#[serial]
async fn removing_next_up_repreloads() {
    let td = TestDaemon::new().await;
    let client = InProcessClient::new(td.core.clone());
    set_playing(&td, songs("t", 3), 0).await; // t-0 playing, t-1 next, t-2 after

    client
        .request(DaemonRequest::RemoveFromQueue(1)) // remove the next-up track
        .await
        .unwrap();

    assert!(
        preloaded(&td, "t-2").await,
        "removing the next-up track must re-preload the new next; commands: {:?}",
        td.fake_mpv.commands().await
    );
}

#[tokio::test]
#[serial]
async fn moving_a_track_into_next_slot_repreloads() {
    let td = TestDaemon::new().await;
    set_playing(&td, songs("t", 3), 0).await; // t-0 playing, t-1 next

    td.core.move_queue_item(2, 1).await; // move t-2 into the next slot

    assert!(
        preloaded(&td, "t-2").await,
        "moving a track into the next slot must re-preload it; commands: {:?}",
        td.fake_mpv.commands().await
    );
}

#[tokio::test]
#[serial]
async fn moving_far_from_the_playhead_does_not_repreload() {
    let td = TestDaemon::new().await;
    set_playing(&td, songs("t", 5), 0).await; // playing t-0, next t-1

    td.core.move_queue_item(3, 2).await; // entirely past the next slot

    let preloaded = td.fake_mpv.commands().await.iter().any(|c| {
        c.first().and_then(Value::as_str) == Some("loadfile")
            && c.get(2).and_then(Value::as_str) == Some("append")
    });
    assert!(
        !preloaded,
        "moving items away from the playhead must not re-preload; commands: {:?}",
        td.fake_mpv.commands().await
    );
}

#[tokio::test]
#[serial]
async fn shuffle_queue_repreloads_the_new_next() {
    let td = TestDaemon::new().await;
    set_playing(&td, songs("t", 4), 0).await;

    td.core.shuffle_queue().await;

    let saw_append = td
        .fake_mpv
        .wait_for(2000, |cmds| {
            cmds.iter().any(|c| {
                c.first().and_then(Value::as_str) == Some("loadfile")
                    && c.get(2).and_then(Value::as_str) == Some("append")
            })
        })
        .await;
    assert!(
        saw_append,
        "shuffle must re-preload the new next track; commands: {:?}",
        td.fake_mpv.commands().await
    );
}
