//! update_playback_info integration: distinguishes AdvanceEarly (replace,
//! queue advances) from Preload (append, queue holds), pins has_next at the
//! last track, and confirms the Continue tail fetches audio properties.
//! Kills gather_playback_tick_inputs + tick_fetch survivors that the existing
//! poll tests leave alive (loadfile t-1 appears in both advance and preload).

mod common;

use common::{songs, TestDaemon};
use ferrosonic::config::RepeatMode;
use ferrosonic::daemon::state::PlaybackState;
use serde_json::json;
use serial_test::serial;

#[tokio::test]
#[serial]
async fn advance_early_advances_the_queue_not_just_preloads() {
    // time_remaining = 2.5 - 1.0 = 1.5 (in the 0..2 window). The tr `-`->`+`/`/`
    // and has_next `<`->`==`/`>` mutants fall to Preload, which holds queue_position at 0.
    let td = TestDaemon::new().await;
    td.fake_subsonic.expect_ping().await;
    {
        let mut s = td.state.write().await;
        s.queue = songs("t", 2);
        s.queue_position = Some(0);
        s.now_playing.song = Some(s.queue[0].clone());
        s.now_playing.state = PlaybackState::Playing;
        s.now_playing.duration = 2.5;
        s.now_playing.position = 1.0;
        s.config.repeat_mode = RepeatMode::Off;
    }
    td.fake_mpv.set_loaded_file("local.mp3").await;

    td.core.update_playback_info().await;

    assert_eq!(
        td.state.read().await.queue_position,
        Some(1),
        "AdvanceEarly must advance the queue to 1, not merely preload (which holds 0)"
    );
}

#[tokio::test]
#[serial]
async fn no_early_advance_at_the_last_track() {
    // At the last position has_next is false (pos+1 == len), so no AdvanceEarly:
    // the tick stays Playing. has_next `<`->`<=` or `+`->`*` would read has_next
    // true at the last track and advance off the end into Stopped.
    let td = TestDaemon::new().await;
    {
        let mut s = td.state.write().await;
        s.queue = songs("t", 2);
        s.queue_position = Some(1);
        s.now_playing.song = Some(s.queue[1].clone());
        s.now_playing.state = PlaybackState::Playing;
        s.now_playing.duration = 2.5;
        s.now_playing.position = 1.0;
        s.config.repeat_mode = RepeatMode::Off;
        s.config.auto_continue = false;
    }
    td.fake_mpv.set_loaded_file("local.mp3").await;

    td.core.update_playback_info().await;

    assert_eq!(
        td.state.read().await.now_playing.state,
        PlaybackState::Playing,
        "no next track means no early advance; state must stay Playing"
    );
}

#[tokio::test]
#[serial]
async fn continue_tick_fetches_audio_properties_when_sample_rate_missing() {
    // A Continue tick (mid-track, count 2, not idle) runs the tail updates,
    // which fetch the sample rate when it is missing. tick_fetch -> () skips it.
    let td = TestDaemon::new().await;
    {
        let mut s = td.state.write().await;
        s.queue = songs("t", 2);
        s.queue_position = Some(0);
        s.now_playing.song = Some(s.queue[0].clone());
        s.now_playing.state = PlaybackState::Playing;
        s.now_playing.duration = 100.0;
        s.now_playing.position = 50.0;
        s.now_playing.sample_rate = None;
    }
    td.fake_mpv.set_loaded_file("local.mp3").await;
    td.fake_mpv
        .set_playlist(vec!["local.mp3".into(), "next.mp3".into()])
        .await;
    td.fake_mpv
        .set_property("audio-params/samplerate", json!(48000))
        .await;

    td.core.update_playback_info().await;

    assert_eq!(
        td.state.read().await.now_playing.sample_rate,
        Some(48000),
        "a Continue tick must backfill the missing sample rate from mpv"
    );
}
