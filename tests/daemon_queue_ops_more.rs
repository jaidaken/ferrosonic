//! queue_ops behaviour: move_queue_item adjusts queue_position correctly across
//! the moved range, shuffle_queue keeps the current track in place, and
//! shuffle_library replaces the queue only with a non-empty batch.

mod common;

use common::{songs, TestDaemon};
use serial_test::serial;

async fn td_with_queue(n: usize, pos: usize) -> TestDaemon {
    let td = TestDaemon::new().await;
    {
        let mut s = td.state.write().await;
        s.queue = songs("q", n);
        s.queue_position = Some(pos);
    }
    td
}

#[tokio::test]
#[serial]
async fn move_from_before_to_after_current_decrements_position() {
    let td = td_with_queue(5, 2).await;
    td.core.move_queue_item(0, 3).await;
    assert_eq!(
        td.state.read().await.queue_position,
        Some(1),
        "moving an item from before the current track to after it shifts cur down"
    );
}

#[tokio::test]
#[serial]
async fn move_from_after_to_before_current_increments_position() {
    let td = td_with_queue(5, 2).await;
    td.core.move_queue_item(4, 1).await;
    assert_eq!(
        td.state.read().await.queue_position,
        Some(3),
        "moving an item from after the current track to before it shifts cur up"
    );
}

#[tokio::test]
#[serial]
async fn moving_the_current_track_makes_position_follow() {
    let td = td_with_queue(5, 2).await;
    td.core.move_queue_item(2, 4).await;
    assert_eq!(
        td.state.read().await.queue_position,
        Some(4),
        "queue_position follows the current track to its new index"
    );
}

#[tokio::test]
#[serial]
async fn move_not_crossing_current_leaves_position() {
    let td = td_with_queue(6, 2).await;
    td.core.move_queue_item(4, 5).await;
    assert_eq!(
        td.state.read().await.queue_position,
        Some(2),
        "a move entirely after the current track leaves the position"
    );
}

#[tokio::test]
#[serial]
async fn shuffle_queue_keeps_the_current_track_in_place() {
    let td = td_with_queue(10, 3).await;
    let current_id = td.state.read().await.queue[3].id.clone();
    td.core.shuffle_queue().await;
    let s = td.state.read().await;
    assert_eq!(s.queue.len(), 10, "shuffle preserves every song");
    assert_eq!(
        s.queue[3].id, current_id,
        "the currently-playing track stays at its index"
    );
}

#[tokio::test]
#[serial]
async fn shuffle_library_replaces_queue_with_random_batch() {
    let td = TestDaemon::new().await;
    td.fake_subsonic.expect_random_songs(&["A", "B", "C"]).await;
    td.core.shuffle_library().await.unwrap();
    assert_eq!(
        td.state.read().await.queue.len(),
        3,
        "shuffle_library fills the queue from the random batch"
    );
}

#[tokio::test]
#[serial]
async fn shuffle_library_with_no_songs_leaves_queue() {
    let td = TestDaemon::new().await;
    td.fake_subsonic.expect_random_songs(&[]).await;
    {
        let mut s = td.state.write().await;
        s.queue = songs("keep", 2);
    }
    td.core.shuffle_library().await.unwrap();
    assert_eq!(
        td.state.read().await.queue.len(),
        2,
        "an empty random batch leaves the existing queue untouched"
    );
}
