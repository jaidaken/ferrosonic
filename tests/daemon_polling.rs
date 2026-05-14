//! daemon/core.rs: update_playback_info polling branches.

mod common;

use common::{song, TestDaemon};
use ferrosonic::daemon::state::PlaybackState;
use ferrosonic::config::RepeatMode;
use serial_test::serial;

#[tokio::test]
#[serial]
async fn update_playback_info_when_paused_runs_position_branch() {
    let td = TestDaemon::new().await;
    {
        let mut s = td.state.write().await;
        s.now_playing.state = PlaybackState::Paused;
        s.now_playing.duration = 60.0;
        s.now_playing.position = 10.0;
        s.queue.push(song("a", "A"));
        s.queue_position = Some(0);
    }
    td.core.update_playback_info().await;
}

#[tokio::test]
#[serial]
async fn update_playback_info_when_playing_with_position_tick() {
    let td = TestDaemon::new().await;
    {
        let mut s = td.state.write().await;
        s.now_playing.state = PlaybackState::Playing;
        s.now_playing.duration = 60.0;
        s.now_playing.position = 5.0;
        s.queue.push(song("a", "A"));
        s.queue_position = Some(0);
    }
    td.core.update_playback_info().await;
}

#[tokio::test]
#[serial]
async fn update_playback_info_near_end_with_has_next_advances_early() {
    let td = TestDaemon::new().await;
    {
        let mut s = td.state.write().await;
        s.now_playing.state = PlaybackState::Playing;
        s.now_playing.duration = 60.0;
        s.now_playing.position = 59.5;
        s.queue = vec![song("a", "A"), song("b", "B")];
        s.queue_position = Some(0);
    }
    td.core.update_playback_info().await;
}

#[tokio::test]
#[serial]
async fn update_playback_info_with_duration_missing_fetches_duration() {
    let td = TestDaemon::new().await;
    {
        let mut s = td.state.write().await;
        s.now_playing.state = PlaybackState::Playing;
        s.now_playing.duration = 0.0;
        s.now_playing.position = 0.0;
        s.queue.push(song("a", "A"));
        s.queue_position = Some(0);
    }
    td.core.update_playback_info().await;
}

#[tokio::test]
#[serial]
async fn update_playback_info_fetches_audio_properties_when_sample_rate_missing() {
    let td = TestDaemon::new().await;
    {
        let mut s = td.state.write().await;
        s.now_playing.state = PlaybackState::Playing;
        s.now_playing.duration = 200.0;
        s.now_playing.position = 50.0;
        s.now_playing.sample_rate = None;
        s.queue.push(song("a", "A"));
        s.queue_position = Some(0);
    }
    td.core.update_playback_info().await;
}

#[tokio::test]
#[serial]
async fn update_playback_info_with_repeat_one_at_end_replays() {
    let td = TestDaemon::new().await;
    {
        let mut s = td.state.write().await;
        s.now_playing.state = PlaybackState::Playing;
        s.now_playing.duration = 200.0;
        s.now_playing.position = 150.0;
        s.queue.push(song("only", "Only"));
        s.queue_position = Some(0);
        s.config.repeat_mode = RepeatMode::One;
    }
    td.core.update_playback_info().await;
}

#[tokio::test]
#[serial]
async fn update_playback_info_with_repeat_all_at_end_wraps() {
    let td = TestDaemon::new().await;
    {
        let mut s = td.state.write().await;
        s.now_playing.state = PlaybackState::Playing;
        s.now_playing.duration = 200.0;
        s.now_playing.position = 150.0;
        s.queue = vec![song("a", "A"), song("b", "B")];
        s.queue_position = Some(1);
        s.config.repeat_mode = RepeatMode::All;
    }
    td.core.update_playback_info().await;
}

#[tokio::test]
#[serial]
async fn preload_next_track_with_song_at_end_uses_repeat_logic() {
    let td = TestDaemon::new().await;
    {
        let mut s = td.state.write().await;
        s.queue = vec![song("a", "A"), song("b", "B")];
        s.queue_position = Some(1);
        s.config.repeat_mode = RepeatMode::All;
    }
    td.core.preload_next_track(1).await;
}

#[tokio::test]
#[serial]
async fn preload_next_track_with_repeat_off_at_end_is_noop() {
    let td = TestDaemon::new().await;
    {
        let mut s = td.state.write().await;
        s.queue = vec![song("a", "A")];
        s.queue_position = Some(0);
        s.config.repeat_mode = RepeatMode::Off;
    }
    td.core.preload_next_track(0).await;
}

#[tokio::test]
#[serial]
async fn snapshot_includes_current_state() {
    let td = TestDaemon::new().await;
    {
        let mut s = td.state.write().await;
        s.queue.push(song("a", "A"));
        s.queue_position = Some(0);
    }
    let snap = td.core.snapshot().await;
    assert_eq!(snap.queue.len(), 1);
    assert_eq!(snap.queue_position, Some(0));
}
