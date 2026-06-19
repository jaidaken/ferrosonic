//! The PipeWire force-rate pin must be cleared when playback leaves
//! Playing (pause/stop) so the audio device can re-rate to other apps,
//! and re-applied when a track plays.

mod common;

use std::time::Duration;

use common::{songs, RecordingPwRunner, TestDaemon};
use ferrosonic::daemon::core::PlayMode;
use ferrosonic::daemon::state::PlaybackState;
use serde_json::json;
use serial_test::serial;
use tokio::time::timeout;

const OP: Duration = Duration::from_secs(5);

/// Poll the recorded `clock.force-rate` writes until `want` shows up; the
/// pin-on-play happens in a background probe task, so it is not synchronous.
async fn wait_for_force_rate(pw: &RecordingPwRunner, want: &str) -> bool {
    timeout(OP, async {
        loop {
            if pw.force_rate_values().iter().any(|v| v == want) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .is_ok()
}

#[tokio::test]
#[serial]
async fn pause_keeps_force_rate_pin() {
    let (td, pw) = TestDaemon::new_with_pw_recorder().await;
    {
        let mut s = td.state.write().await;
        s.queue = songs("t", 1);
        s.queue_position = Some(0);
        s.now_playing.state = PlaybackState::Playing;
        s.now_playing.sample_rate = Some(44_100);
    }

    timeout(OP, td.core.pause_playback())
        .await
        .expect("pause did not hang")
        .unwrap();

    assert!(
        pw.force_rate_values().is_empty(),
        "pause must keep the pin (issue no force-rate change) so resume is gapless"
    );
}

#[tokio::test]
#[serial]
async fn stop_releases_force_rate_pin() {
    let (td, pw) = TestDaemon::new_with_pw_recorder().await;
    {
        let mut s = td.state.write().await;
        s.queue = songs("t", 1);
        s.queue_position = Some(0);
        s.now_playing.state = PlaybackState::Playing;
        s.now_playing.sample_rate = Some(48_000);
    }

    timeout(OP, td.core.stop_playback())
        .await
        .expect("stop did not hang")
        .unwrap();

    assert_eq!(
        pw.force_rate_values(),
        vec!["0".to_string()],
        "stop must clear the PipeWire force-rate to 0"
    );
}

#[tokio::test]
#[serial]
async fn play_pins_rate_then_pause_keeps_it() {
    let (td, pw) = TestDaemon::new_with_pw_recorder().await;
    td.fake_mpv
        .set_property("audio-params/samplerate", json!(44_100))
        .await;
    {
        let mut s = td.state.write().await;
        s.queue = songs("t", 1);
    }

    timeout(OP, td.core.play_queue_position(0, PlayMode::Direct))
        .await
        .expect("play did not hang")
        .unwrap();

    assert!(
        wait_for_force_rate(&pw, "44100").await,
        "play must pin the force-rate to the track's 44100 rate"
    );

    timeout(OP, td.core.pause_playback())
        .await
        .expect("pause did not hang")
        .unwrap();

    assert_eq!(
        pw.force_rate_values().last().map(String::as_str),
        Some("44100"),
        "pause keeps the pin at the track rate (released only on stop)"
    );
}
