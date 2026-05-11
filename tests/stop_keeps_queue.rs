//! Regression tests for the 0.4.1 MPRIS Stop fix.
//!
//! Stop must halt playback while leaving the queue and current
//! selection intact. Play after Stop must resume the same track from
//! frame 0 by re-issuing play_queue_position.

mod common;

use common::{song, TestDaemon};
use ferrosonic::app::state::PlaybackState;
use serde_json::Value;
use serial_test::serial;

#[tokio::test]
#[serial]
async fn stop_preserves_queue_and_position() {
    let td = TestDaemon::new().await;

    // Seed a queue and position state to mimic mid-playback. We don't
    // actually start playback; the test only exercises the Stop
    // mutation against the daemon state.
    {
        let mut s = td.state.write().await;
        s.queue = vec![
            song("a", "Track A"),
            song("b", "Track B"),
            song("c", "Track C"),
        ];
        s.queue_position = Some(1);
        s.now_playing.song = Some(s.queue[1].clone());
        s.now_playing.state = PlaybackState::Playing;
        s.now_playing.position = 42.0;
        s.now_playing.duration = 180.0;
    }

    td.core.stop_keep_queue().await.expect("stop_keep_queue");

    let s = td.state.read().await;
    assert_eq!(s.queue.len(), 3, "Stop must not touch the queue length");
    assert_eq!(s.queue_position, Some(1), "Stop must keep queue_position");
    assert!(
        s.now_playing.song.is_some(),
        "Stop must keep now_playing.song so the UI knows which track is queued"
    );
    assert_eq!(
        s.now_playing.song.as_ref().unwrap().id,
        "b",
        "Stop must keep the same selected track"
    );
    assert_eq!(s.now_playing.state, PlaybackState::Stopped);
    assert_eq!(
        s.now_playing.position, 0.0,
        "Stop must rewind position to 0"
    );
}

#[tokio::test]
#[serial]
async fn resume_from_stopped_replays_via_loadfile() {
    let td = TestDaemon::new().await;
    td.fake_subsonic.expect_ping().await;

    // Seed and Stop.
    {
        let mut s = td.state.write().await;
        s.queue = vec![song("a", "Track A"), song("b", "Track B")];
        s.queue_position = Some(0);
        s.now_playing.song = Some(s.queue[0].clone());
        s.now_playing.state = PlaybackState::Stopped;
        s.now_playing.position = 0.0;
    }

    // Trigger the new Stopped -> Play transition.
    td.core
        .resume_playback()
        .await
        .expect("resume_playback from Stopped");

    // Direct PlayMode in resume routes through play_queue_position
    // which calls mpv.loadfile. The fake should have captured it.
    let saw_loadfile = td
        .fake_mpv
        .wait_for(2000, |cmds| {
            cmds.iter().any(|c| {
                c.first().and_then(Value::as_str) == Some("loadfile")
                    && c.get(1)
                        .and_then(Value::as_str)
                        .map(|p| p.contains("id=a"))
                        .unwrap_or(false)
            })
        })
        .await;

    assert!(
        saw_loadfile,
        "resume_playback from Stopped should fire mpv loadfile for queue[0]; commands seen: {:?}",
        td.fake_mpv.commands().await
    );
}

#[tokio::test]
#[serial]
async fn toggle_pause_from_stopped_also_resumes() {
    let td = TestDaemon::new().await;
    td.fake_subsonic.expect_ping().await;

    {
        let mut s = td.state.write().await;
        s.queue = vec![song("first", "First")];
        s.queue_position = Some(0);
        s.now_playing.song = Some(s.queue[0].clone());
        s.now_playing.state = PlaybackState::Stopped;
    }

    td.core.toggle_pause().await.expect("toggle_pause");

    let saw_loadfile = td
        .fake_mpv
        .wait_for(2000, |cmds| {
            cmds.iter()
                .any(|c| c.first().and_then(Value::as_str) == Some("loadfile"))
        })
        .await;

    assert!(
        saw_loadfile,
        "toggle_pause from Stopped should fire mpv loadfile; commands seen: {:?}",
        td.fake_mpv.commands().await
    );
}

#[tokio::test]
#[serial]
async fn stop_with_empty_queue_is_safe() {
    let td = TestDaemon::new().await;

    // No queue, no song. Stop should be a clean no-op (or set state
    // to Stopped without panicking).
    td.core
        .stop_keep_queue()
        .await
        .expect("stop_keep_queue with empty queue must not error");

    let s = td.state.read().await;
    assert_eq!(s.queue.len(), 0);
    assert_eq!(s.queue_position, None);
    assert_eq!(s.now_playing.state, PlaybackState::Stopped);
}
