//! pause / resume / toggle_pause transitions outside the Stopped path.

mod common;

use common::{songs, TestDaemon};
use ferrosonic::daemon::state::PlaybackState;
use serde_json::Value;
use serial_test::serial;

async fn saw_cmd(td: &TestDaemon, name: &str) -> bool {
    td.fake_mpv
        .commands()
        .await
        .iter()
        .any(|c| c.first().and_then(Value::as_str) == Some(name))
}

#[tokio::test]
#[serial]
async fn pause_from_playing_stops_mpv_to_free_device() {
    let td = TestDaemon::new().await;
    {
        let mut s = td.state.write().await;
        s.queue = songs("t", 1);
        s.queue_position = Some(0);
        s.now_playing.state = PlaybackState::Playing;
    }

    td.core.pause_playback().await.unwrap();

    assert_eq!(
        td.state.read().await.now_playing.state,
        PlaybackState::Paused
    );
    assert!(
        saw_cmd(&td, "stop").await,
        "pause must stop mpv so it releases the audio device"
    );
}

#[tokio::test]
#[serial]
async fn pause_when_not_playing_is_noop() {
    let td = TestDaemon::new().await;
    {
        let mut s = td.state.write().await;
        s.now_playing.state = PlaybackState::Paused;
    }

    td.core.pause_playback().await.unwrap();

    assert!(
        !saw_cmd(&td, "stop").await,
        "pause from a non-Playing state must not stop mpv"
    );
}

#[tokio::test]
#[serial]
async fn resume_from_paused_reloads_and_plays() {
    let td = TestDaemon::new().await;
    {
        let mut s = td.state.write().await;
        s.queue = songs("t", 1);
        s.queue_position = Some(0);
        s.now_playing.state = PlaybackState::Paused;
    }

    td.core.resume_playback().await.unwrap();

    assert_eq!(
        td.state.read().await.now_playing.state,
        PlaybackState::Playing
    );
    assert!(
        saw_cmd(&td, "loadfile").await,
        "resume from Paused must reload the track via mpv loadfile"
    );
}

#[tokio::test]
#[serial]
async fn toggle_pause_cycles_playing_to_paused_to_playing() {
    let td = TestDaemon::new().await;
    {
        let mut s = td.state.write().await;
        s.queue = songs("t", 1);
        s.queue_position = Some(0);
        s.now_playing.state = PlaybackState::Playing;
    }

    td.core.toggle_pause().await.unwrap();
    assert_eq!(
        td.state.read().await.now_playing.state,
        PlaybackState::Paused
    );

    td.core.toggle_pause().await.unwrap();
    assert_eq!(
        td.state.read().await.now_playing.state,
        PlaybackState::Playing
    );
}

#[tokio::test]
#[serial]
async fn prev_from_index_zero_under_repeat_all_wraps_to_last() {
    let td = TestDaemon::new().await;
    td.fake_subsonic.expect_ping().await;
    {
        let mut s = td.state.write().await;
        s.queue = songs("t", 4);
        s.queue_position = Some(0);
        s.now_playing.position = 1.0;
        s.config.repeat_mode = ferrosonic::config::RepeatMode::All;
    }

    td.core.prev_track().await.unwrap();

    let loadfiles: Vec<String> = td
        .fake_mpv
        .commands()
        .await
        .iter()
        .filter(|c| c.first().and_then(Value::as_str) == Some("loadfile"))
        .filter_map(|c| c.get(1).and_then(Value::as_str).map(String::from))
        .collect();
    assert!(
        loadfiles.iter().any(|p| p.contains("id=t-3")),
        "prev at index 0 under repeat-All must wrap to last track; loadfiles: {:?}",
        loadfiles
    );
}
