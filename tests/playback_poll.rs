//! `update_playback_info` polling cycle: position tick, idle-end advance,
//! near-end early advance, gapless advance via playlist-pos.

mod common;

use common::{song, songs, TestDaemon};
use ferrosonic::daemon::state::PlaybackState;
use serde_json::Value;
use serial_test::serial;

async fn loadfile_paths(td: &TestDaemon) -> Vec<String> {
    td.fake_mpv
        .commands()
        .await
        .iter()
        .filter(|c| c.first().and_then(Value::as_str) == Some("loadfile"))
        .filter_map(|c| c.get(1).and_then(Value::as_str).map(String::from))
        .collect()
}

#[tokio::test]
#[serial]
async fn position_tick_updates_state_when_playing() {
    let td = TestDaemon::new().await;
    {
        let mut s = td.state.write().await;
        s.queue.push(song("a", "A"));
        s.queue_position = Some(0);
        s.now_playing.song = Some(song("a", "A"));
        s.now_playing.state = PlaybackState::Playing;
        s.now_playing.duration = 200.0;
    }
    td.fake_mpv.set_loaded_file("local.mp3").await;
    td.fake_mpv.set_position(42.0).await;
    td.fake_mpv
        .set_playlist(vec!["local.mp3".into(), "local2.mp3".into()])
        .await;

    td.core.update_playback_info().await;

    let s = td.state.read().await;
    assert!(
        (s.now_playing.position - 42.0).abs() < 1.0,
        "expected ~42s, got {}",
        s.now_playing.position
    );
}

#[tokio::test]
#[serial]
async fn idle_state_triggers_advance_auto() {
    let td = TestDaemon::new().await;
    td.fake_subsonic.expect_ping().await;
    {
        let mut s = td.state.write().await;
        s.queue = songs("t", 3);
        s.queue_position = Some(0);
        s.now_playing.song = Some(s.queue[0].clone());
        s.now_playing.state = PlaybackState::Playing;
        s.now_playing.duration = 100.0;
    }
    // fake mpv: no file loaded -> idle-active true. Also playlist
    // must be empty for the advance path; set_loaded_file would
    // make it non-empty. Skip loading.

    td.core.update_playback_info().await;

    // advance_auto with Off repeat + position 0 should move to 1.
    let loads = loadfile_paths(&td).await;
    assert!(
        loads.iter().any(|p| p.contains("id=t-1")),
        "idle should trigger advance to next track; loadfiles: {:?}",
        loads
    );
}

#[tokio::test]
#[serial]
async fn near_end_of_track_with_no_preload_calls_next_track() {
    let td = TestDaemon::new().await;
    td.fake_subsonic.expect_ping().await;
    {
        let mut s = td.state.write().await;
        s.queue = songs("t", 3);
        s.queue_position = Some(0);
        s.now_playing.song = Some(s.queue[0].clone());
        s.now_playing.state = PlaybackState::Playing;
        s.now_playing.duration = 100.0;
        s.now_playing.position = 99.0;
    }
    td.fake_mpv.set_loaded_file("local.mp3").await;
    td.fake_mpv.set_playlist(vec!["local.mp3".into()]).await;

    td.core.update_playback_info().await;

    let loads = loadfile_paths(&td).await;
    assert!(
        loads.iter().any(|p| p.contains("id=t-1")),
        "near-end with playlist-count=1 should call next_track; loads: {:?}",
        loads
    );
}

#[tokio::test]
#[serial]
async fn playlist_pos_one_indicates_gapless_advance() {
    let td = TestDaemon::new().await;
    td.fake_subsonic.expect_ping().await;
    {
        let mut s = td.state.write().await;
        s.queue = songs("t", 3);
        s.queue_position = Some(0);
        s.now_playing.song = Some(s.queue[0].clone());
        s.now_playing.state = PlaybackState::Playing;
        s.now_playing.duration = 100.0;
        s.now_playing.position = 50.0;
    }
    td.fake_mpv.set_loaded_file("local.mp3").await;
    td.fake_mpv
        .set_playlist(vec!["local.mp3".into(), "local2.mp3".into()])
        .await;
    td.fake_mpv.set_playlist_pos(1).await;

    td.core.update_playback_info().await;

    let s = td.state.read().await;
    assert_eq!(
        s.queue_position,
        Some(1),
        "playlist-pos=1 must bump queue_position from 0 to 1"
    );
    assert_eq!(
        s.now_playing.song.as_ref().map(|s| s.id.as_str()),
        Some("t-1"),
        "now_playing.song must update to queue[1]"
    );
}

#[tokio::test]
#[serial]
async fn poll_when_not_playing_returns_early() {
    let td = TestDaemon::new().await;
    {
        let mut s = td.state.write().await;
        s.queue.push(song("a", "A"));
        s.now_playing.state = PlaybackState::Stopped;
    }

    td.core.update_playback_info().await;
    let cmds = td.fake_mpv.commands().await;
    assert!(
        cmds.is_empty(),
        "Stopped state must not poll mpv; saw: {:?}",
        cmds
    );
}
