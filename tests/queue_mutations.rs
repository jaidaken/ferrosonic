//! Queue mutation invariants.

mod common;

use common::{song, songs, TestDaemon};
use serial_test::serial;

#[tokio::test]
#[serial]
async fn move_item_before_current_decrements_position() {
    let td = TestDaemon::new().await;
    {
        let mut s = td.state.write().await;
        s.queue = songs("t", 5);
        s.queue_position = Some(3);
    }

    td.core.move_queue_item(1, 4).await;

    let s = td.state.read().await;
    assert_eq!(
        s.queue.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(),
        vec!["t-0", "t-2", "t-3", "t-4", "t-1"]
    );
    assert_eq!(
        s.queue_position,
        Some(2),
        "current shifts left when an earlier item moves past it"
    );
}

#[tokio::test]
#[serial]
async fn move_item_after_current_increments_position() {
    let td = TestDaemon::new().await;
    {
        let mut s = td.state.write().await;
        s.queue = songs("t", 5);
        s.queue_position = Some(2);
    }

    td.core.move_queue_item(4, 0).await;

    let s = td.state.read().await;
    assert_eq!(
        s.queue.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(),
        vec!["t-4", "t-0", "t-1", "t-2", "t-3"]
    );
    assert_eq!(
        s.queue_position,
        Some(3),
        "current shifts right when a later item moves past it"
    );
}

#[tokio::test]
#[serial]
async fn move_current_follows_to_new_index() {
    let td = TestDaemon::new().await;
    {
        let mut s = td.state.write().await;
        s.queue = songs("t", 5);
        s.queue_position = Some(2);
    }

    td.core.move_queue_item(2, 4).await;

    let s = td.state.read().await;
    assert_eq!(
        s.queue.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(),
        vec!["t-0", "t-1", "t-3", "t-4", "t-2"]
    );
    assert_eq!(
        s.queue_position,
        Some(4),
        "current follows itself when moved"
    );
}

#[tokio::test]
#[serial]
async fn move_with_out_of_range_indices_is_noop() {
    let td = TestDaemon::new().await;
    {
        let mut s = td.state.write().await;
        s.queue = songs("t", 3);
        s.queue_position = Some(1);
    }

    td.core.move_queue_item(10, 0).await;
    td.core.move_queue_item(0, 10).await;
    td.core.move_queue_item(1, 1).await;

    let s = td.state.read().await;
    assert_eq!(
        s.queue.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(),
        vec!["t-0", "t-1", "t-2"]
    );
    assert_eq!(s.queue_position, Some(1));
}

#[tokio::test]
#[serial]
async fn shuffle_preserves_current_track_in_place() {
    let td = TestDaemon::new().await;
    {
        let mut s = td.state.write().await;
        s.queue = songs("t", 8);
        s.queue_position = Some(3);
    }
    let current_id = td.state.read().await.queue[3].id.clone();

    for _ in 0..5 {
        td.core.shuffle_queue().await;
        let s = td.state.read().await;
        assert_eq!(s.queue.len(), 8, "shuffle must not lose songs");
        assert_eq!(
            s.queue[3].id, current_id,
            "shuffle must keep the current track at its index"
        );
        let mut ids: Vec<_> = s.queue.iter().map(|c| c.id.clone()).collect();
        ids.sort();
        assert_eq!(
            ids,
            (0..8).map(|i| format!("t-{}", i)).collect::<Vec<_>>(),
            "shuffle must preserve the original song set"
        );
    }
}

#[tokio::test]
#[serial]
async fn shuffle_empty_queue_is_safe() {
    let td = TestDaemon::new().await;
    td.core.shuffle_queue().await;
    let s = td.state.read().await;
    assert!(s.queue.is_empty());
}

#[tokio::test]
#[serial]
async fn clear_history_drains_before_current() {
    let td = TestDaemon::new().await;
    {
        let mut s = td.state.write().await;
        s.queue = songs("t", 6);
        s.queue_position = Some(3);
    }

    let removed = td.core.clear_queue_history().await;

    assert_eq!(removed, 3);
    let s = td.state.read().await;
    assert_eq!(
        s.queue.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(),
        vec!["t-3", "t-4", "t-5"]
    );
    assert_eq!(s.queue_position, Some(0));
}

#[tokio::test]
#[serial]
async fn clear_history_at_zero_position_is_noop() {
    let td = TestDaemon::new().await;
    {
        let mut s = td.state.write().await;
        s.queue = songs("t", 3);
        s.queue_position = Some(0);
    }

    let removed = td.core.clear_queue_history().await;

    assert_eq!(removed, 0);
    let s = td.state.read().await;
    assert_eq!(s.queue.len(), 3);
    assert_eq!(s.queue_position, Some(0));
}

#[tokio::test]
#[serial]
async fn clear_history_without_position_is_noop() {
    let td = TestDaemon::new().await;
    {
        let mut s = td.state.write().await;
        s.queue = vec![song("a", "A")];
        s.queue_position = None;
    }

    let removed = td.core.clear_queue_history().await;

    assert_eq!(removed, 0);
}
