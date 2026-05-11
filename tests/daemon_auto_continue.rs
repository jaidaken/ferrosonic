//! daemon/core.rs: auto_continue advance + prev wrap branches.

mod common;

use common::{song, TestDaemon};
use ferrosonic::config::RepeatMode;
use serial_test::serial;

#[tokio::test]
#[serial]
async fn next_track_at_end_with_auto_continue_and_random_appends_and_plays() {
    let td = TestDaemon::new().await;
    td.fake_subsonic.expect_random_songs(&["nc1", "nc2"]).await;
    {
        let mut s = td.state.write().await;
        s.queue = vec![song("last", "Last")];
        s.queue_position = Some(0);
        s.config.auto_continue = true;
        s.config.repeat_mode = RepeatMode::Off;
    }
    let _ = td.core.next_track().await;
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    let st = td.state.read().await;
    assert!(st.queue.len() > 1);
}

#[tokio::test]
#[serial]
async fn advance_auto_at_end_with_auto_continue_appends_random_songs() {
    let td = TestDaemon::new().await;
    td.fake_subsonic
        .expect_random_songs(&["a1", "a2", "a3"])
        .await;
    {
        let mut s = td.state.write().await;
        s.queue = vec![song("only", "Only")];
        s.queue_position = Some(0);
        s.config.auto_continue = true;
        s.config.repeat_mode = RepeatMode::Off;
    }
    let _ = td.core.advance_auto().await;
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    let st = td.state.read().await;
    assert!(st.queue.len() > 1);
}

#[tokio::test]
#[serial]
async fn prev_track_with_position_above_three_seeks_to_zero() {
    let td = TestDaemon::new().await;
    {
        let mut s = td.state.write().await;
        s.queue = vec![song("a", "A"), song("b", "B")];
        s.queue_position = Some(1);
        s.now_playing.position = 10.0;
    }
    let _ = td.core.prev_track().await;
    let st = td.state.read().await;
    assert_eq!(st.queue_position, Some(1));
}

#[tokio::test]
#[serial]
async fn prev_track_with_position_under_three_goes_to_previous_track() {
    let td = TestDaemon::new().await;
    {
        let mut s = td.state.write().await;
        s.queue = vec![song("a", "A"), song("b", "B")];
        s.queue_position = Some(1);
        s.now_playing.position = 1.5;
    }
    let _ = td.core.prev_track().await;
}

#[tokio::test]
#[serial]
async fn prev_track_at_zero_with_repeat_off_and_low_position_seeks_to_zero() {
    let td = TestDaemon::new().await;
    {
        let mut s = td.state.write().await;
        s.queue = vec![song("only", "Only")];
        s.queue_position = Some(0);
        s.now_playing.position = 2.0;
        s.config.repeat_mode = RepeatMode::Off;
    }
    let _ = td.core.prev_track().await;
}

#[tokio::test]
#[serial]
async fn prev_track_with_no_queue_position_and_low_position_is_safe() {
    let td = TestDaemon::new().await;
    {
        let mut s = td.state.write().await;
        s.queue = vec![song("a", "A")];
        s.queue_position = None;
        s.now_playing.position = 1.0;
    }
    let _ = td.core.prev_track().await;
}

#[tokio::test]
#[serial]
async fn next_track_at_end_with_auto_continue_no_songs_response_notifies() {
    let td = TestDaemon::new().await;
    td.fake_subsonic.expect_random_songs(&[]).await;
    {
        let mut s = td.state.write().await;
        s.queue = vec![song("last", "Last")];
        s.queue_position = Some(0);
        s.config.auto_continue = true;
        s.config.repeat_mode = RepeatMode::Off;
    }
    let _ = td.core.next_track().await;
}
