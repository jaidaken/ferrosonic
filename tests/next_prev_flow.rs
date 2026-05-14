//! Integration tests for `next_track` and `prev_track` against the
//! fake mpv. Verifies repeat-mode integration and gapless emit.

mod common;

use common::{songs, TestDaemon};
use ferrosonic::daemon::state::PlaybackState;
use ferrosonic::config::RepeatMode;
use serde_json::Value;
use serial_test::serial;

async fn loadfile_paths(td: &TestDaemon) -> Vec<String> {
    td.fake_mpv
        .commands()
        .await
        .iter()
        .filter(|c| c.first().and_then(Value::as_str) == Some("loadfile"))
        .filter_map(|c| c.get(1).and_then(Value::as_str).map(String::from))
        .collect()
}

#[tokio::test]
#[serial]
async fn next_advances_one_step_under_repeat_off() {
    let td = TestDaemon::new().await;
    td.fake_subsonic.expect_ping().await;
    {
        let mut s = td.state.write().await;
        s.queue = songs("t", 3);
        s.queue_position = Some(0);
        s.config.repeat_mode = RepeatMode::Off;
    }

    td.core.next_track().await.unwrap();

    let loads = loadfile_paths(&td).await;
    assert!(
        loads.iter().any(|p| p.contains("id=t-1")),
        "next under Off must load index 1; loadfiles: {:?}",
        loads
    );
}

#[tokio::test]
#[serial]
async fn next_at_end_under_repeat_off_stops_playback() {
    let td = TestDaemon::new().await;
    {
        let mut s = td.state.write().await;
        s.queue = songs("t", 3);
        s.queue_position = Some(2);
        s.config.repeat_mode = RepeatMode::Off;
        s.config.auto_continue = false;
    }

    td.core.next_track().await.unwrap();

    let s = td.state.read().await;
    assert_eq!(
        s.now_playing.state,
        PlaybackState::Stopped,
        "next at end with Off + no auto-continue must stop"
    );
}

#[tokio::test]
#[serial]
async fn next_at_end_under_repeat_all_wraps_to_first() {
    let td = TestDaemon::new().await;
    td.fake_subsonic.expect_ping().await;
    {
        let mut s = td.state.write().await;
        s.queue = songs("t", 3);
        s.queue_position = Some(2);
        s.config.repeat_mode = RepeatMode::All;
    }

    td.core.next_track().await.unwrap();

    let loads = loadfile_paths(&td).await;
    assert!(
        loads.iter().any(|p| p.contains("id=t-0")),
        "next at end under All must wrap to first track; loadfiles: {:?}",
        loads
    );
}

#[tokio::test]
#[serial]
async fn manual_next_under_repeat_one_still_advances() {
    let td = TestDaemon::new().await;
    td.fake_subsonic.expect_ping().await;
    {
        let mut s = td.state.write().await;
        s.queue = songs("t", 3);
        s.queue_position = Some(0);
        s.config.repeat_mode = RepeatMode::One;
    }

    td.core.next_track().await.unwrap();

    let loads = loadfile_paths(&td).await;
    assert!(
        loads.iter().any(|p| p.contains("id=t-1")),
        "manual Next under repeat-One must still move forward (auto-advance is what repeats)"
    );
}

#[tokio::test]
#[serial]
async fn prev_advances_back_one_when_near_start_of_track() {
    let td = TestDaemon::new().await;
    td.fake_subsonic.expect_ping().await;
    {
        let mut s = td.state.write().await;
        s.queue = songs("t", 3);
        s.queue_position = Some(2);
        s.now_playing.position = 1.0;
    }

    td.core.prev_track().await.unwrap();

    let loads = loadfile_paths(&td).await;
    assert!(
        loads.iter().any(|p| p.contains("id=t-1")),
        "prev with position < 3s should step back one track"
    );
}

#[tokio::test]
#[serial]
async fn prev_with_deep_position_restarts_current_track() {
    let td = TestDaemon::new().await;
    {
        let mut s = td.state.write().await;
        s.queue = songs("t", 3);
        s.queue_position = Some(2);
        s.now_playing.position = 30.0;
    }

    td.core.prev_track().await.unwrap();

    let saw_seek_zero = td.fake_mpv.commands().await.iter().any(|c| {
        c.first().and_then(Value::as_str) == Some("seek")
            && c.get(1).and_then(Value::as_f64) == Some(0.0)
    });
    assert!(
        saw_seek_zero,
        "prev with position > 3s should seek to 0 instead of changing track"
    );
}
