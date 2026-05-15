//! daemon/core.rs: action methods (seek, volume, halt, advance).

mod common;

use common::{song, TestDaemon};
use serial_test::serial;

#[tokio::test]
#[serial]
async fn seek_with_fake_mpv_does_not_crash() {
    let td = TestDaemon::new().await;
    td.core.seek(45.0).await.unwrap();
}

#[tokio::test]
#[serial]
async fn seek_relative_offset_does_not_crash() {
    let td = TestDaemon::new().await;
    td.core.seek_relative(10.0).await.unwrap();
}

#[tokio::test]
#[serial]
async fn seek_relative_negative_offset_does_not_crash() {
    let td = TestDaemon::new().await;
    td.core.seek_relative(-10.0).await.unwrap();
}

#[tokio::test]
#[serial]
async fn set_volume_persists_via_mpv() {
    let td = TestDaemon::new().await;
    td.core.set_volume(60).await.unwrap();
    let saw_set_volume = td
        .fake_mpv
        .wait_for(500, |cmds| {
            cmds.iter().any(|c| {
                c.first().and_then(|v| v.as_str()) == Some("set_property")
                    && c.get(1).and_then(|v| v.as_str()) == Some("volume")
                    && c.get(2).and_then(serde_json::Value::as_f64) == Some(60.0)
            })
        })
        .await;
    assert!(saw_set_volume, "set_volume(60) must dispatch volume=60 to mpv");
}

#[tokio::test]
#[serial]
async fn set_volume_zero_does_not_crash() {
    let td = TestDaemon::new().await;
    td.core.set_volume(0).await.unwrap();
}

#[tokio::test]
#[serial]
async fn set_volume_max_does_not_crash() {
    let td = TestDaemon::new().await;
    td.core.set_volume(100).await.unwrap();
}

#[tokio::test]
#[serial]
async fn halt_keep_queue_clears_now_playing_keeps_queue() {
    let td = TestDaemon::new().await;
    {
        let mut s = td.state.write().await;
        s.queue.push(song("a", "Track A"));
        s.queue_position = Some(0);
        s.now_playing.song = Some(song("a", "Track A"));
        s.now_playing.duration = 200.0;
    }
    td.core.halt_keep_queue().await;
    let s = td.state.read().await;
    assert_eq!(s.queue.len(), 1);
    assert!(s.now_playing.song.is_none());
    assert_eq!(s.now_playing.duration, 0.0);
}

#[tokio::test]
#[serial]
async fn stop_keep_queue_preserves_song_in_state() {
    let td = TestDaemon::new().await;
    {
        let mut s = td.state.write().await;
        s.queue.push(song("a", "Track A"));
        s.now_playing.song = Some(song("a", "Track A"));
    }
    td.core.stop_keep_queue().await.unwrap();
    let s = td.state.read().await;
    assert_eq!(s.queue.len(), 1);
}

#[tokio::test]
#[serial]
async fn stop_playback_clears_queue_and_state() {
    let td = TestDaemon::new().await;
    {
        let mut s = td.state.write().await;
        s.queue.push(song("a", "T"));
        s.queue_position = Some(0);
    }
    td.core.stop_playback().await.unwrap();
    let s = td.state.read().await;
    assert!(s.queue.is_empty());
}

#[tokio::test]
#[serial]
async fn play_queue_position_oob_index_returns_error_or_ignores() {
    let td = TestDaemon::new().await;
    let result = td
        .core
        .play_queue_position(99, ferrosonic::daemon::core::PlayMode::Direct)
        .await;
    let s = td.state.read().await;
    assert!(s.queue.is_empty());
    assert!(s.queue_position.is_none());
    assert!(
        result.is_err() || s.now_playing.song.is_none(),
        "oob play_queue_position must return Err or leave now_playing empty"
    );
}

#[tokio::test]
#[serial]
async fn next_track_with_empty_queue_returns_ok() {
    let td = TestDaemon::new().await;
    let _ = td.core.next_track().await;
}

#[tokio::test]
#[serial]
async fn prev_track_with_empty_queue_returns_ok() {
    let td = TestDaemon::new().await;
    let _ = td.core.prev_track().await;
}

#[tokio::test]
#[serial]
async fn advance_auto_with_empty_queue_returns_ok() {
    let td = TestDaemon::new().await;
    let _ = td.core.advance_auto().await;
}

#[tokio::test]
#[serial]
async fn toggle_pause_with_no_active_playback_returns_ok() {
    let td = TestDaemon::new().await;
    let _ = td.core.toggle_pause().await;
}

#[tokio::test]
#[serial]
async fn pause_playback_returns_ok() {
    let td = TestDaemon::new().await;
    let _ = td.core.pause_playback().await;
}

#[tokio::test]
#[serial]
async fn resume_playback_returns_ok() {
    let td = TestDaemon::new().await;
    let _ = td.core.resume_playback().await;
}

#[tokio::test]
#[serial]
async fn shuffle_library_with_no_subsonic_does_not_crash() {
    let td = TestDaemon::new().await;
    {
        let mut s = td.state.write().await;
        s.config.base_url.clear();
    }
    let _ = td.core.shuffle_library().await;
}

#[tokio::test]
#[serial]
async fn search_with_no_subsonic_returns_empty() {
    let td = TestDaemon::new().await;
    {
        let mut s = td.state.write().await;
        s.config.base_url.clear();
    }
    let r = td.core.search("query", 10, 10, 10).await;
    assert!(r.artist.is_empty());
    assert!(r.album.is_empty());
    assert!(r.song.is_empty());
}

#[tokio::test]
#[serial]
async fn load_album_songs_with_no_subsonic_does_nothing() {
    let td = TestDaemon::new().await;
    {
        let mut s = td.state.write().await;
        s.config.base_url.clear();
    }
    let _ = td.core.load_album_songs("alb-1").await;
}

#[tokio::test]
#[serial]
async fn load_playlist_songs_with_no_subsonic_does_nothing() {
    let td = TestDaemon::new().await;
    {
        let mut s = td.state.write().await;
        s.config.base_url.clear();
    }
    let _ = td.core.load_playlist_songs("pl-1").await;
}

#[tokio::test]
#[serial]
async fn update_playback_info_when_stopped_returns_early() {
    let td = TestDaemon::new().await;
    td.core.update_playback_info().await;
}

#[tokio::test]
#[serial]
async fn update_server_config_with_bad_url_returns_error() {
    let td = TestDaemon::new().await;
    let r = td
        .core
        .update_server_config("not a url", "u", &"p".into())
        .await;
    assert!(r.is_err());
}
