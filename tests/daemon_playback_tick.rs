//! update_playback_info integration: a track with no preload plays to its end
//! (the queue holds until mpv goes idle), the last track stays Playing, and
//! the Continue tail fetches audio properties.

mod common;

use common::{songs, TestDaemon};
use ferrosonic::config::RepeatMode;
use ferrosonic::daemon::state::PlaybackState;
use serde_json::json;
use serial_test::serial;

#[tokio::test]
#[serial]
async fn near_end_without_a_preload_holds_the_queue_on_the_playing_track() {
    // 1.5 s remain and mpv still plays. Advancing here would cut the tail;
    // the end-of-file listener or the idle tick advances at the true end.
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
        Some(0),
        "the playing track keeps its last 1.5 s; the queue holds at 0"
    );
}

#[tokio::test]
#[serial]
async fn no_early_advance_at_the_last_track() {
    // The last track near its end must stay Playing until mpv goes idle.
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
        "a playing last track is not stopped early; state must stay Playing"
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
