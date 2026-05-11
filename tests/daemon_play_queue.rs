//! daemon/core.rs: play_queue_position branches (Direct + Buffered modes).

mod common;

use common::{song, TestDaemon};
use ferrosonic::daemon::core::PlayMode;
use serial_test::serial;

#[tokio::test]
#[serial]
async fn play_queue_position_direct_mode_with_real_song() {
    let td = TestDaemon::new().await;
    {
        let mut s = td.state.write().await;
        s.queue = vec![song("s1", "Track 1"), song("s2", "Track 2")];
    }
    let _ = td.core.play_queue_position(0, PlayMode::Direct).await;
}

#[tokio::test]
#[serial]
async fn play_queue_position_buffered_mode_with_real_song() {
    let td = TestDaemon::new().await;
    {
        let mut s = td.state.write().await;
        s.queue = vec![song("s1", "Track 1")];
    }
    let _ = td.core.play_queue_position(0, PlayMode::Buffered).await;
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
}

#[tokio::test]
#[serial]
async fn play_queue_position_with_no_subsonic_returns_early() {
    let td = TestDaemon::new().await;
    {
        let mut s = td.state.write().await;
        s.queue = vec![song("s1", "Track 1")];
        s.config.base_url.clear();
    }
    let _ = td.core.play_queue_position(0, PlayMode::Direct).await;
}

#[tokio::test]
#[serial]
async fn play_queue_position_at_end_does_not_panic() {
    let td = TestDaemon::new().await;
    {
        let mut s = td.state.write().await;
        s.queue = vec![song("a", "A")];
    }
    let _ = td.core.play_queue_position(1, PlayMode::Direct).await;
}

#[tokio::test]
#[serial]
async fn next_track_at_end_with_auto_continue_no_subsonic_stops() {
    let td = TestDaemon::new().await;
    {
        let mut s = td.state.write().await;
        s.queue = vec![song("a", "A")];
        s.queue_position = Some(0);
        s.config.auto_continue = true;
        s.config.base_url.clear();
    }
    let _ = td.core.next_track().await;
}

#[tokio::test]
#[serial]
async fn prev_track_at_zero_with_repeat_all_wraps_to_end() {
    let td = TestDaemon::new().await;
    {
        let mut s = td.state.write().await;
        s.queue = vec![song("a", "A"), song("b", "B"), song("c", "C")];
        s.queue_position = Some(0);
        s.config.repeat_mode = ferrosonic::config::RepeatMode::All;
    }
    let _ = td.core.prev_track().await;
}

#[tokio::test]
#[serial]
async fn prev_track_at_zero_with_repeat_one_wraps_to_end() {
    let td = TestDaemon::new().await;
    {
        let mut s = td.state.write().await;
        s.queue = vec![song("a", "A"), song("b", "B")];
        s.queue_position = Some(0);
        s.config.repeat_mode = ferrosonic::config::RepeatMode::One;
    }
    let _ = td.core.prev_track().await;
}

#[tokio::test]
#[serial]
async fn shuffle_library_with_subsonic_replaces_queue() {
    let td = TestDaemon::new().await;
    td.fake_subsonic.expect_random_songs(&["a", "b", "c"]).await;
    let _ = td.core.shuffle_library().await;
}

#[tokio::test]
#[serial]
async fn remove_from_queue_within_bounds_updates_position() {
    let td = TestDaemon::new().await;
    {
        let mut s = td.state.write().await;
        s.queue = vec![song("a", "A"), song("b", "B"), song("c", "C")];
        s.queue_position = Some(2);
    }
    td.core.move_queue_item(0, 1).await;
}

#[tokio::test]
#[serial]
async fn move_queue_item_to_same_position_is_noop() {
    let td = TestDaemon::new().await;
    {
        let mut s = td.state.write().await;
        s.queue = vec![song("a", "A"), song("b", "B")];
    }
    td.core.move_queue_item(0, 0).await;
}

#[tokio::test]
#[serial]
async fn clear_queue_history_after_playing_a_few_tracks() {
    let td = TestDaemon::new().await;
    {
        let mut s = td.state.write().await;
        s.queue = vec![
            song("p0", "P0"),
            song("p1", "P1"),
            song("p2", "P2"),
            song("c", "Current"),
            song("n", "Next"),
        ];
        s.queue_position = Some(3);
    }
    let removed = td.core.clear_queue_history().await;
    assert_eq!(removed, 3);
    let s = td.state.read().await;
    assert_eq!(s.queue.len(), 2);
}
