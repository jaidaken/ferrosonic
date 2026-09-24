//! Bit-perfect rate-switch pre-roll: a track loads paused, the audio device
//! re-clocks during that silence, then playback unpauses, so a sample-rate
//! change lands in the pre-roll gap and never in the first frames of music.

mod common;

use std::time::Duration;

use common::{songs, TestDaemon};
use ferrosonic::daemon::core::PlayMode;
use ferrosonic::secret::Secret;
use ferrosonic::subsonic::client::SubsonicClient;
use serde_json::{json, Value};
use serial_test::serial;
use tokio::time::timeout;

const OP: Duration = Duration::from_secs(5);

/// Settle long enough that "waited" and "skipped the wait" land in disjoint
/// windows: a skipped settle unpauses within one 30 ms probe of the rate
/// appearing, a kept one cannot unpause before this many ms have passed.
const LONG_SETTLE_MS: u32 = 1500;
/// Window checked for a premature unpause. A kept settle can never unpause
/// inside it, whatever the machine load, so it cannot flake green-side.
const EARLY_WINDOW_MS: u64 = 700;

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

/// True once mpv received an unpause after the most recent replacing load.
fn unpaused_after_paused_load(cmds: &[Vec<Value>]) -> bool {
    let Some(load) = cmds.iter().rposition(|c| is_loadfile_replace(c)) else {
        return false;
    };
    cmds.iter().skip(load).any(|c| is_set_pause(c, false))
}

/// Pin 96 kHz the way the fast probe or the tick backstop does, after the
/// paused load and before the settle's probe sees the decoded rate, then
/// require the settle to still hold the track paused for the full delay.
async fn assert_settle_survives_a_foreign_pin(mode: PlayMode) {
    let (td, pw) = TestDaemon::new_with_pw_recorder().await;
    td.fake_subsonic
        .expect_stream_for("t-0", vec![0u8; 700 * 1024])
        .await;
    {
        let mut s = td.state.write().await;
        s.queue = songs("t", 1);
        s.config.rate_switch_delay_ms = LONG_SETTLE_MS;
    }

    timeout(OP, td.core.play_queue_position(0, mode))
        .await
        .expect("play did not hang")
        .unwrap();
    let loaded = td
        .fake_mpv
        .wait_for(5000, |cmds| cmds.iter().any(|c| is_loadfile_replace(c)))
        .await;
    assert!(loaded, "the track reached mpv as a replacing load");

    td.core
        .pipewire
        .lock()
        .await
        .set_rate(96_000)
        .await
        .unwrap();
    td.fake_mpv
        .set_property("audio-params/samplerate", json!(96_000))
        .await;

    let early = td
        .fake_mpv
        .wait_for(EARLY_WINDOW_MS, unpaused_after_paused_load)
        .await;
    assert!(
        !early,
        "a rate pinned by another path during the pre-roll must not skip the \
         settle; mpv log: {:?}",
        td.fake_mpv.commands().await
    );
    let late = td.fake_mpv.wait_for(5000, unpaused_after_paused_load).await;
    assert!(late, "the settle unpauses once the delay has passed");
    assert_eq!(
        pw.force_rate_values().last().map(String::as_str),
        Some("96000"),
        "the graph ends pinned to the track's rate"
    );
}

#[tokio::test]
#[serial]
async fn direct_play_keeps_the_settle_when_another_path_pins_the_rate_first() {
    assert_settle_survives_a_foreign_pin(PlayMode::Direct).await;
}

#[tokio::test]
#[serial]
async fn album_play_keeps_the_settle_when_another_path_pins_the_rate_first() {
    assert_settle_survives_a_foreign_pin(PlayMode::Buffered).await;
}

#[tokio::test]
#[serial]
async fn same_rate_track_unpauses_without_waiting_the_settle() {
    let (td, _pw) = TestDaemon::new_with_pw_recorder().await;
    td.fake_mpv
        .set_property("audio-params/samplerate", json!(44_100))
        .await;
    {
        let mut s = td.state.write().await;
        s.queue = songs("t", 1);
        s.config.rate_switch_delay_ms = LONG_SETTLE_MS;
    }
    td.core
        .pipewire
        .lock()
        .await
        .set_rate(44_100)
        .await
        .unwrap();

    timeout(OP, td.core.play_queue_position(0, PlayMode::Direct))
        .await
        .expect("play did not hang")
        .unwrap();

    let prompt = td
        .fake_mpv
        .wait_for(EARLY_WINDOW_MS, unpaused_after_paused_load)
        .await;
    assert!(
        prompt,
        "a track at the already-pinned rate starts without the settle delay"
    );
}

#[tokio::test]
#[serial]
async fn prebuffer_failure_fallback_still_loads_paused_behind_the_settle() {
    let (td, pw) = TestDaemon::new_with_pw_recorder().await;
    // Port 1 refuses the connection, so the prebuffer fetch takes its fallback.
    let dead = SubsonicClient::new("http://127.0.0.1:1", "test", &Secret::from("test")).unwrap();
    *td.core.subsonic.write().await = Some(dead);
    td.fake_mpv
        .set_property("audio-params/samplerate", json!(96_000))
        .await;
    {
        let mut s = td.state.write().await;
        s.queue = songs("t", 1);
    }

    timeout(OP, td.core.play_queue_position(0, PlayMode::Buffered))
        .await
        .expect("play did not hang")
        .unwrap();

    let unpaused = td.fake_mpv.wait_for(5000, unpaused_after_paused_load).await;
    assert!(
        unpaused,
        "the fallback load starts playback after its settle"
    );
    let cmds = td.fake_mpv.commands().await;
    let load = cmds.iter().rposition(|c| is_loadfile_replace(c));
    let pause = cmds.iter().rposition(|c| is_set_pause(c, true));
    let (Some(ld), Some(pt)) = (load, pause) else {
        panic!("expected pause-true then loadfile in mpv log: {cmds:?}");
    };
    assert!(
        pt < ld,
        "the fallback stream loads paused, so a rate switch lands in silence"
    );
    assert!(
        pw.force_rate_values().iter().any(|v| v == "96000"),
        "the fallback path pins the decoded rate before playback"
    );
}
