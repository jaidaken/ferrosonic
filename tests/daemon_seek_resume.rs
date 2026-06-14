//! Seek, relative-seek, resume-at-offset, and the prev_track 3s restart
//! boundary. Kills playback_ops survivors that exercise these lines without
//! asserting the position write or the mpv seek command.

mod common;

use common::{songs, TestDaemon};
use ferrosonic::config::RepeatMode;
use ferrosonic::daemon::core::PlayMode;
use ferrosonic::daemon::state::PlaybackState;
use serde_json::Value;
use serial_test::serial;

fn loadfile_paths(cmds: &[Vec<Value>]) -> Vec<String> {
    cmds.iter()
        .filter(|c| c.first().and_then(Value::as_str) == Some("loadfile"))
        .filter_map(|c| c.get(1).and_then(Value::as_str).map(String::from))
        .collect()
}

#[tokio::test]
#[serial]
async fn prev_at_exactly_three_seconds_restarts_current_track() {
    // The `position < 3.0` boundary: at exactly 3.0 prev restarts (seek 0), it
    // does not step back. `<`->`<=` would step back to the previous track.
    let td = TestDaemon::new().await;
    td.fake_subsonic.expect_ping().await;
    {
        let mut s = td.state.write().await;
        s.queue = songs("t", 3);
        s.queue_position = Some(2);
        s.now_playing.position = 3.0;
        s.config.repeat_mode = RepeatMode::Off;
    }

    td.core.prev_track().await.unwrap();

    let cmds = td.fake_mpv.commands().await;
    let saw_seek_zero = cmds.iter().any(|c| {
        c.first().and_then(Value::as_str) == Some("seek")
            && c.get(1).and_then(Value::as_f64) == Some(0.0)
    });
    assert!(saw_seek_zero, "prev at exactly 3.0s must seek to 0 (restart)");
    assert!(
        !loadfile_paths(&cmds).iter().any(|p| p.contains("id=t-1")),
        "prev at exactly 3.0s must not step back to the previous track"
    );
}

#[tokio::test]
#[serial]
async fn resume_at_offset_commits_the_offset_to_now_playing_position() {
    // `start_at > 0.0` mutated to `== 0.0` / `< 0.0` skips the commit, leaving
    // position at the 0.0 set during commit_play_state_in_lock.
    let td = TestDaemon::new().await;
    {
        let mut s = td.state.write().await;
        s.queue = songs("t", 1);
        s.queue_position = Some(0);
    }

    td.core
        .play_queue_position_at(0, PlayMode::Direct, 7.5)
        .await
        .unwrap();

    assert_eq!(
        td.state.read().await.now_playing.position,
        7.5,
        "resuming at an offset must commit that offset to now_playing.position"
    );
}

#[tokio::test]
#[serial]
async fn seek_sends_absolute_command_and_commits_position() {
    let td = TestDaemon::new().await;
    {
        let mut s = td.state.write().await;
        s.queue = songs("t", 1);
        s.queue_position = Some(0);
        s.now_playing.state = PlaybackState::Playing;
    }

    td.core.seek(10.0).await.unwrap();

    let saw_absolute_seek = td.fake_mpv.commands().await.iter().any(|c| {
        c.first().and_then(Value::as_str) == Some("seek")
            && c.get(1).and_then(Value::as_f64) == Some(10.0)
            && c.get(2).and_then(Value::as_str) == Some("absolute")
    });
    assert!(saw_absolute_seek, "seek must send an absolute seek to mpv");
    assert_eq!(
        td.state.read().await.now_playing.position,
        10.0,
        "seek must commit the target position to now_playing.position"
    );
}

#[tokio::test]
#[serial]
async fn seek_relative_sends_relative_command_with_the_offset() {
    let td = TestDaemon::new().await;
    {
        let mut s = td.state.write().await;
        s.queue = songs("t", 1);
        s.queue_position = Some(0);
        s.now_playing.state = PlaybackState::Playing;
    }

    td.core.seek_relative(5.0).await.unwrap();

    let saw_relative_seek = td.fake_mpv.commands().await.iter().any(|c| {
        c.first().and_then(Value::as_str) == Some("seek")
            && c.get(1).and_then(Value::as_f64) == Some(5.0)
            && c.get(2).and_then(Value::as_str) == Some("relative")
    });
    assert!(
        saw_relative_seek,
        "seek_relative must send a relative seek with the offset to mpv"
    );
}
