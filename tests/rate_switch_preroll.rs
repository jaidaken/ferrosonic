//! Bit-perfect rate-switch pre-roll: a track loads paused, the audio device
//! re-clocks during that silence, then playback unpauses, so a sample-rate
//! change lands in the pre-roll gap and never in the first frames of music.

mod common;

use std::time::Duration;

use common::{songs, TestDaemon};
use ferrosonic::daemon::core::PlayMode;
use serde_json::{json, Value};
use serial_test::serial;
use tokio::time::timeout;

const OP: Duration = Duration::from_secs(5);

fn is_set_pause(cmd: &[Value], want: bool) -> bool {
    matches!(cmd, [c, p, v]
        if c == "set_property" && p == "pause" && v.as_bool() == Some(want))
}

fn is_loadfile_replace(cmd: &[Value]) -> bool {
    cmd.first().and_then(Value::as_str) == Some("loadfile")
        && cmd.get(2).and_then(Value::as_str) != Some("append")
}

#[tokio::test]
#[serial]
async fn track_loads_paused_then_unpauses_to_hide_the_switch() {
    let (td, _pw) = TestDaemon::new_with_pw_recorder().await;
    td.fake_mpv
        .set_property("audio-params/samplerate", json!(96_000))
        .await;
    {
        let mut s = td.state.write().await;
        s.queue = songs("t", 1);
    }

    timeout(OP, td.core.play_queue_position(0, PlayMode::Direct))
        .await
        .expect("play did not hang")
        .unwrap();

    // The settle runs in a spawned task; wait for the unpause it issues.
    let unpaused = td
        .fake_mpv
        .wait_for(5000, |cmds| cmds.iter().any(|c| is_set_pause(c, false)))
        .await;
    assert!(unpaused, "the spawned settle unpaused the track");

    let cmds = td.fake_mpv.commands().await;
    let pause_true = cmds.iter().position(|c| is_set_pause(c, true));
    let load = cmds.iter().position(|c| is_loadfile_replace(c));
    let pause_false = cmds.iter().rposition(|c| is_set_pause(c, false));
    let (Some(pt), Some(ld), Some(pf)) = (pause_true, load, pause_false) else {
        panic!("expected pause-true, loadfile, pause-false in mpv log: {cmds:?}");
    };
    assert!(
        pt < ld,
        "the track is paused before it is loaded (load-paused)"
    );
    assert!(ld < pf, "playback unpauses only after the paused load");
}

#[tokio::test]
#[serial]
async fn rate_change_is_pinned_before_playback_unpauses() {
    let (td, pw) = TestDaemon::new_with_pw_recorder().await;
    td.fake_mpv
        .set_property("audio-params/samplerate", json!(96_000))
        .await;
    {
        let mut s = td.state.write().await;
        s.queue = songs("t", 1);
    }

    timeout(OP, td.core.play_queue_position(0, PlayMode::Direct))
        .await
        .expect("play did not hang")
        .unwrap();

    // The settle sets the rate then unpauses, both sequential awaits, so once
    // the unpause is observed the 96k pin is already recorded.
    let unpaused = td
        .fake_mpv
        .wait_for(5000, |cmds| cmds.iter().any(|c| is_set_pause(c, false)))
        .await;
    assert!(unpaused, "settle unpaused the track");

    assert!(
        pw.force_rate_values().iter().any(|v| v == "96000"),
        "the decoded 96k rate was pinned before unpausing, not after"
    );
    let s = td.state.read().await;
    assert_eq!(
        s.now_playing.sample_rate,
        Some(96_000),
        "now-playing reflects the probed rate"
    );
}
