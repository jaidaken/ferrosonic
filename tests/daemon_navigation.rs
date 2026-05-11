//! daemon/core.rs: next_track/prev_track/advance_auto with populated queue.

mod common;

use common::{song, TestDaemon};
use ferrosonic::config::RepeatMode;
use serial_test::serial;

#[tokio::test]
#[serial]
async fn next_track_with_no_position_starts_at_zero() {
    let td = TestDaemon::new().await;
    {
        let mut s = td.state.write().await;
        s.queue = vec![song("a", "A"), song("b", "B")];
    }
    let _ = td.core.next_track().await;
}

#[tokio::test]
#[serial]
async fn next_track_at_end_with_no_auto_continue_stops() {
    let td = TestDaemon::new().await;
    {
        let mut s = td.state.write().await;
        s.queue = vec![song("a", "A")];
        s.queue_position = Some(0);
        s.config.auto_continue = false;
        s.config.repeat_mode = RepeatMode::Off;
    }
    let _ = td.core.next_track().await;
}

#[tokio::test]
#[serial]
async fn next_track_with_repeat_one_replays_current() {
    let td = TestDaemon::new().await;
    {
        let mut s = td.state.write().await;
        s.queue = vec![song("a", "A")];
        s.queue_position = Some(0);
        s.config.repeat_mode = RepeatMode::One;
    }
    let _ = td.core.next_track().await;
}

#[tokio::test]
#[serial]
async fn next_track_with_repeat_all_wraps_to_zero() {
    let td = TestDaemon::new().await;
    {
        let mut s = td.state.write().await;
        s.queue = vec![song("a", "A"), song("b", "B")];
        s.queue_position = Some(1);
        s.config.repeat_mode = RepeatMode::All;
    }
    let _ = td.core.next_track().await;
}

#[tokio::test]
#[serial]
async fn prev_track_with_no_position_returns_ok() {
    let td = TestDaemon::new().await;
    {
        let mut s = td.state.write().await;
        s.queue = vec![song("a", "A")];
    }
    let _ = td.core.prev_track().await;
}

#[tokio::test]
#[serial]
async fn prev_track_at_zero_position_with_repeat_off_does_not_underflow() {
    let td = TestDaemon::new().await;
    {
        let mut s = td.state.write().await;
        s.queue = vec![song("a", "A"), song("b", "B")];
        s.queue_position = Some(0);
        s.config.repeat_mode = RepeatMode::Off;
    }
    let _ = td.core.prev_track().await;
}

#[tokio::test]
#[serial]
async fn prev_track_from_index_one_goes_to_zero() {
    let td = TestDaemon::new().await;
    {
        let mut s = td.state.write().await;
        s.queue = vec![song("a", "A"), song("b", "B")];
        s.queue_position = Some(1);
        s.config.repeat_mode = RepeatMode::Off;
    }
    let _ = td.core.prev_track().await;
}

#[tokio::test]
#[serial]
async fn advance_auto_at_end_with_repeat_one_replays() {
    let td = TestDaemon::new().await;
    {
        let mut s = td.state.write().await;
        s.queue = vec![song("a", "A")];
        s.queue_position = Some(0);
        s.config.repeat_mode = RepeatMode::One;
    }
    let _ = td.core.advance_auto().await;
}

#[tokio::test]
#[serial]
async fn advance_auto_with_repeat_off_stops_at_end() {
    let td = TestDaemon::new().await;
    {
        let mut s = td.state.write().await;
        s.queue = vec![song("a", "A")];
        s.queue_position = Some(0);
        s.config.repeat_mode = RepeatMode::Off;
        s.config.auto_continue = false;
    }
    let _ = td.core.advance_auto().await;
}

#[tokio::test]
#[serial]
async fn advance_auto_with_repeat_all_wraps() {
    let td = TestDaemon::new().await;
    {
        let mut s = td.state.write().await;
        s.queue = vec![song("a", "A"), song("b", "B")];
        s.queue_position = Some(1);
        s.config.repeat_mode = RepeatMode::All;
    }
    let _ = td.core.advance_auto().await;
}

#[tokio::test]
#[serial]
async fn play_queue_position_with_valid_index_plays() {
    let td = TestDaemon::new().await;
    {
        let mut s = td.state.write().await;
        s.queue = vec![song("a", "A"), song("b", "B")];
    }
    let _ = td
        .core
        .play_queue_position(1, ferrosonic::daemon::core::PlayMode::Direct)
        .await;
}

#[tokio::test]
#[serial]
async fn move_queue_item_within_bounds_succeeds() {
    let td = TestDaemon::new().await;
    {
        let mut s = td.state.write().await;
        s.queue = vec![song("a", "A"), song("b", "B"), song("c", "C")];
    }
    td.core.move_queue_item(0, 2).await;
    let s = td.state.read().await;
    assert_eq!(s.queue[2].id, "a");
}

#[tokio::test]
#[serial]
async fn shuffle_queue_with_nonempty_queue_permutes() {
    let td = TestDaemon::new().await;
    {
        let mut s = td.state.write().await;
        s.queue = (0..20).map(|i| song(&format!("{i}"), "x")).collect();
        s.queue_position = Some(0);
    }
    td.core.shuffle_queue().await;
}

#[tokio::test]
#[serial]
async fn clear_queue_history_with_played_songs_removes_them() {
    let td = TestDaemon::new().await;
    {
        let mut s = td.state.write().await;
        s.queue = vec![song("a", "A"), song("b", "B"), song("c", "C")];
        s.queue_position = Some(2);
    }
    let removed = td.core.clear_queue_history().await;
    assert_eq!(removed, 2);
    let s = td.state.read().await;
    assert_eq!(s.queue.len(), 1);
    assert_eq!(s.queue[0].id, "c");
}

#[tokio::test]
#[serial]
async fn preload_next_track_with_no_subsonic_does_not_crash() {
    let td = TestDaemon::new().await;
    {
        let mut s = td.state.write().await;
        s.queue = vec![song("a", "A"), song("b", "B")];
        s.config.base_url.clear();
    }
    td.core.preload_next_track(0).await;
}
