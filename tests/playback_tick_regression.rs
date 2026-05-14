//! Regression coverage for the split update_playback_info: assert observable state transitions per action variant.

mod common;

use common::{song, TestDaemon};
use ferrosonic::daemon::state::PlaybackState;
use serde_json::json;
use serial_test::serial;

#[tokio::test]
#[serial]
async fn skip_path_does_not_mutate_state() {
    let td = TestDaemon::new().await;
    {
        let mut s = td.state.write().await;
        s.now_playing.state = PlaybackState::Stopped;
        s.now_playing.position = 7.5;
        s.now_playing.duration = 200.0;
    }
    let before_position = td.state.read().await.now_playing.position;
    td.core.update_playback_info().await;
    let after_position = td.state.read().await.now_playing.position;
    assert_eq!(before_position, after_position);
}

#[tokio::test]
#[serial]
async fn paused_path_runs_tail_updates_and_syncs_position() {
    let td = TestDaemon::new().await;
    td.fake_mpv.set_loaded_file("/fake/song-a.flac").await;
    td.fake_mpv.set_position(42.0).await;
    {
        let mut s = td.state.write().await;
        s.now_playing.state = PlaybackState::Paused;
        s.now_playing.duration = 100.0;
        s.now_playing.position = 0.0;
        s.queue.push(song("a", "A"));
        s.queue_position = Some(0);
    }
    td.core.update_playback_info().await;
    assert!((td.state.read().await.now_playing.position - 42.0).abs() < 0.01);
}

#[tokio::test]
#[serial]
async fn duration_backfill_writes_only_when_missing() {
    let td = TestDaemon::new().await;
    td.fake_mpv.set_loaded_file("/fake/song-a.flac").await;
    td.fake_mpv.set_duration(180.0).await;
    {
        let mut s = td.state.write().await;
        s.now_playing.state = PlaybackState::Playing;
        s.now_playing.duration = 0.0;
        s.now_playing.position = 0.0;
        s.queue.push(song("a", "A"));
        s.queue_position = Some(0);
    }
    td.core.update_playback_info().await;
    let dur = td.state.read().await.now_playing.duration;
    assert!(dur > 0.0, "duration backfill did not fire: {}", dur);
}

#[tokio::test]
#[serial]
async fn duration_backfill_does_not_overwrite_existing() {
    let td = TestDaemon::new().await;
    td.fake_mpv.set_loaded_file("/fake/song-a.flac").await;
    td.fake_mpv.set_duration(180.0).await;
    {
        let mut s = td.state.write().await;
        s.now_playing.state = PlaybackState::Playing;
        s.now_playing.duration = 99.5;
        s.now_playing.position = 0.0;
        s.queue.push(song("a", "A"));
        s.queue_position = Some(0);
    }
    td.core.update_playback_info().await;
    assert!((td.state.read().await.now_playing.duration - 99.5).abs() < 0.01);
}

#[tokio::test]
#[serial]
async fn near_end_with_empty_playlist_does_not_panic() {
    let td = TestDaemon::new().await;
    td.fake_mpv.set_loaded_file("/fake/song-a.flac").await;
    td.fake_mpv.set_playlist(vec!["/fake/song-a.flac".into()]).await;
    {
        let mut s = td.state.write().await;
        s.now_playing.state = PlaybackState::Playing;
        s.now_playing.duration = 60.0;
        s.now_playing.position = 59.0;
        s.queue = vec![song("a", "A"), song("b", "B")];
        s.queue_position = Some(0);
    }
    td.core.update_playback_info().await;
}

#[tokio::test]
#[serial]
async fn idle_path_with_loaded_file_skips_advance() {
    let td = TestDaemon::new().await;
    td.fake_mpv.set_loaded_file("/fake/song-a.flac").await;
    td.fake_mpv.set_property("idle-active", json!(false)).await;
    {
        let mut s = td.state.write().await;
        s.now_playing.state = PlaybackState::Playing;
        s.now_playing.duration = 100.0;
        s.now_playing.position = 30.0;
        s.queue.push(song("a", "A"));
        s.queue_position = Some(0);
    }
    let before_qp = td.state.read().await.queue_position;
    td.core.update_playback_info().await;
    assert_eq!(td.state.read().await.queue_position, before_qp);
}
