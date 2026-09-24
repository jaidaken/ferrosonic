//! Gapless hand-off across a sample-rate change: mpv keeps the first file's
//! output format for a gapless next, then the rate pin re-clocks the device
//! mid-music. A cross-rate next is never preloaded, so the track ends, the
//! next one loads paused and the re-clock lands in the pre-roll silence.

mod common;

use common::{songs, TestDaemon};
use ferrosonic::config::RepeatMode;
use ferrosonic::daemon::state::PlaybackState;
use serde_json::Value;
use serial_test::serial;

fn appends(cmds: &[Vec<Value>]) -> usize {
    cmds.iter()
        .filter(|c| {
            c.first().and_then(Value::as_str) == Some("loadfile")
                && c.get(2).and_then(Value::as_str) == Some("append")
        })
        .count()
}

/// Queue of two songs at the given source rates, current at position 0.
async fn two_track_queue(td: &TestDaemon, first: Option<i32>, second: Option<i32>) {
    let mut s = td.state.write().await;
    s.queue = songs("t", 2);
    s.queue[0].sampling_rate = first;
    s.queue[1].sampling_rate = second;
    s.queue_position = Some(0);
    s.config.repeat_mode = RepeatMode::Off;
}

#[tokio::test]
#[serial]
async fn cross_rate_next_track_is_not_preloaded() {
    let td = TestDaemon::new().await;
    two_track_queue(&td, Some(44_100), Some(96_000)).await;
    td.fake_mpv.set_playlist(vec!["current".into()]).await;

    td.core.preload_next_track(0).await;

    assert_eq!(
        appends(&td.fake_mpv.commands().await),
        0,
        "a 44.1k -> 96k hand-off must not go gapless through the re-clock"
    );
}

#[tokio::test]
#[serial]
async fn same_rate_next_track_still_preloads() {
    let td = TestDaemon::new().await;
    two_track_queue(&td, Some(96_000), Some(96_000)).await;
    td.fake_mpv.set_playlist(vec!["current".into()]).await;

    td.core.preload_next_track(0).await;

    assert_eq!(
        appends(&td.fake_mpv.commands().await),
        1,
        "same-rate tracks keep the gapless preload"
    );
}

#[tokio::test]
#[serial]
async fn unknown_rate_next_track_still_preloads() {
    let td = TestDaemon::new().await;
    two_track_queue(&td, Some(44_100), None).await;
    td.fake_mpv.set_playlist(vec!["current".into()]).await;

    td.core.preload_next_track(0).await;

    assert_eq!(
        appends(&td.fake_mpv.commands().await),
        1,
        "a server without samplingRate keeps the gapless preload"
    );
}

#[tokio::test]
#[serial]
async fn zero_rate_from_the_server_counts_as_unknown_and_still_preloads() {
    // A server that sends samplingRate 0 for a track it cannot probe must not
    // make every hand-off from that track look like a rate switch.
    let td = TestDaemon::new().await;
    two_track_queue(&td, Some(0), Some(96_000)).await;
    td.fake_mpv.set_playlist(vec!["current".into()]).await;

    td.core.preload_next_track(0).await;

    assert_eq!(
        appends(&td.fake_mpv.commands().await),
        1,
        "a 0 Hz sampling rate is unknown metadata, so gapless stays on"
    );
}

#[tokio::test]
#[serial]
async fn cross_rate_near_end_tick_lets_the_track_finish() {
    // 1.5s left and no preload in mpv: without the rate guard the tick
    // advances early, cuts the tail and starts the next track mid re-clock.
    let td = TestDaemon::new().await;
    two_track_queue(&td, Some(44_100), Some(96_000)).await;
    {
        let mut s = td.state.write().await;
        s.now_playing.song = Some(s.queue[0].clone());
        s.now_playing.state = PlaybackState::Playing;
        s.now_playing.duration = 2.5;
        s.now_playing.position = 1.0;
    }
    td.fake_mpv.set_loaded_file("local.flac").await;

    td.core.update_playback_info().await;

    assert_eq!(
        td.state.read().await.queue_position,
        Some(0),
        "the current track plays to its end before a cross-rate next"
    );
}

#[tokio::test]
#[serial]
async fn cross_rate_track_end_advances_on_idle() {
    // mpv ran out (idle) with only the finished entry in its playlist. The
    // tick must advance to the next track, not re-try a preload it skips.
    let td = TestDaemon::new().await;
    two_track_queue(&td, Some(44_100), Some(96_000)).await;
    {
        let mut s = td.state.write().await;
        s.now_playing.song = Some(s.queue[0].clone());
        s.now_playing.state = PlaybackState::Playing;
        // Outside the 2s early-advance window, so only the Preload-vs-idle
        // choice decides this tick.
        s.now_playing.duration = 180.0;
        s.now_playing.position = 170.0;
    }
    td.fake_mpv.set_playlist(vec!["finished".into()]).await;

    td.core.update_playback_info().await;

    assert_eq!(
        td.state.read().await.queue_position,
        Some(1),
        "the idle tick advances to the cross-rate next track"
    );
}
