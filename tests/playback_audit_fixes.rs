//! Track-change and preload defects found by the playback audit: a stale
//! download fallback, an early advance that cut the song tail, stale gapless
//! preloads after queue edits, a resync that removed the playing entry, a
//! double auto-advance, and quality fields that outlived a gapless change.

mod common;

use std::time::Duration;

use common::{songs, TestDaemon};
use ferrosonic::config::RepeatMode;
use ferrosonic::daemon::core::PlayMode;
use ferrosonic::daemon::state::PlaybackState;
use ferrosonic::secret::Secret;
use ferrosonic::subsonic::client::SubsonicClient;
use ferrosonic::subsonic::models::Child;
use serde_json::{json, Value};
use serial_test::serial;

fn named(cmds: &[Vec<Value>], name: &str) -> Vec<Vec<Value>> {
    cmds.iter()
        .filter(|c| c.first().and_then(Value::as_str) == Some(name))
        .cloned()
        .collect()
}

fn appended_ids(cmds: &[Vec<Value>]) -> Vec<String> {
    named(cmds, "loadfile")
        .iter()
        .filter(|c| c.get(2).and_then(Value::as_str) == Some("append"))
        .filter_map(|c| c.get(1).and_then(Value::as_str).map(String::from))
        .collect()
}

async fn playing_at(td: &TestDaemon, n: usize, pos: usize, repeat: RepeatMode) {
    let mut s = td.state.write().await;
    s.queue = songs("t", n);
    s.queue_position = Some(pos);
    s.now_playing.song = Some(s.queue[pos].clone());
    s.now_playing.state = PlaybackState::Playing;
    s.config.repeat_mode = repeat;
}

#[tokio::test]
#[serial]
async fn superseded_album_download_failure_does_not_load_over_the_new_album() {
    let td = TestDaemon::new().await;
    td.fake_subsonic
        .expect_stream_for("t-1", vec![0u8; 64 * 1024])
        .await;
    td.fake_mpv
        .set_property("audio-params/samplerate", json!(44_100))
        .await;
    td.state.write().await.queue = songs("t", 2);
    // Album A's server accepts the request, then drops it after 2 s.
    let lis = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = lis.local_addr().unwrap().port();
    tokio::spawn(async move {
        if let Ok((sock, _)) = lis.accept().await {
            tokio::time::sleep(Duration::from_secs(2)).await;
            drop(sock);
        }
    });
    let good = td.core.subsonic.read().await.clone();
    let dead = SubsonicClient::new(
        &format!("http://127.0.0.1:{port}"),
        "test",
        &Secret::from("test"),
    )
    .unwrap();
    *td.core.subsonic.write().await = Some(dead);
    td.core
        .play_queue_position(0, PlayMode::Buffered)
        .await
        .unwrap();
    // The user picks album B before A's download fails.
    *td.core.subsonic.write().await = good;
    td.core
        .play_queue_position(1, PlayMode::Buffered)
        .await
        .unwrap();

    tokio::time::sleep(Duration::from_secs(5)).await;
    let cmds = td.fake_mpv.commands().await;
    let replacing: Vec<String> = named(&cmds, "loadfile")
        .iter()
        .filter(|c| c.get(2).and_then(Value::as_str) != Some("append"))
        .filter_map(|c| c.get(1).and_then(Value::as_str).map(String::from))
        .collect();
    assert_eq!(td.state.read().await.queue_position, Some(1));
    assert!(
        !replacing.iter().any(|u| u.contains("id=t-0")),
        "the cancelled album A download must not load A; loads: {replacing:?}"
    );
}

#[tokio::test]
#[serial]
async fn song_before_a_radio_station_plays_to_its_end() {
    let td = TestDaemon::new().await;
    td.fake_subsonic.expect_ping().await;
    {
        let mut s = td.state.write().await;
        let mut q = songs("t", 1);
        q.push(Child {
            id: "radio:1".into(),
            title: "Station".into(),
            radio_stream_url: Some("http://r.example/live".into()),
            ..Default::default()
        });
        s.queue = q;
        s.queue_position = Some(0);
        s.now_playing.song = Some(s.queue[0].clone());
        s.now_playing.state = PlaybackState::Playing;
        s.now_playing.duration = 180.0;
        s.now_playing.position = 178.5;
        s.config.repeat_mode = RepeatMode::Off;
    }
    td.fake_mpv.set_loaded_file("song.flac").await;

    td.core.update_playback_info().await;

    assert_eq!(
        td.state.read().await.queue_position,
        Some(0),
        "1.5 s of the song remain, so the tick must not jump to the station"
    );
}

#[tokio::test]
#[serial]
async fn moving_the_wrap_target_on_the_last_track_resyncs_the_preload() {
    let td = TestDaemon::new().await;
    playing_at(&td, 5, 4, RepeatMode::All).await;
    td.fake_mpv
        .set_playlist(vec!["cur".into(), "t-0".into()])
        .await;

    td.core.move_queue_item(0, 2).await;

    let cmds = td.fake_mpv.commands().await;
    assert_eq!(
        named(&cmds, "playlist-remove"),
        vec![vec![json!("playlist-remove"), json!(1)]],
        "the stale t-0 preload is dropped"
    );
    let ids = appended_ids(&cmds);
    assert!(
        ids.last().is_some_and(|u| u.contains("id=t-1")),
        "the new wrap target t-1 is preloaded; appends: {ids:?}"
    );
}

#[tokio::test]
#[serial]
async fn clearing_history_on_the_last_track_resyncs_the_preload() {
    let td = TestDaemon::new().await;
    playing_at(&td, 3, 2, RepeatMode::All).await;
    td.fake_mpv
        .set_playlist(vec!["cur".into(), "t-0".into()])
        .await;

    assert_eq!(td.core.clear_queue_history().await, 2);

    let cmds = td.fake_mpv.commands().await;
    assert_eq!(
        named(&cmds, "playlist-remove"),
        vec![vec![json!("playlist-remove"), json!(1)]],
        "removed t-0 must not stay queued in mpv"
    );
    let ids = appended_ids(&cmds);
    assert!(
        ids.last().is_some_and(|u| u.contains("id=t-2")),
        "repeat All over a 1-track queue preloads the current song; appends: {ids:?}"
    );
}

#[tokio::test]
#[serial]
async fn resync_never_removes_the_entry_mpv_already_plays() {
    let td = TestDaemon::new().await;
    playing_at(&td, 3, 0, RepeatMode::Off).await;
    td.fake_mpv
        .set_playlist(vec!["t-0".into(), "t-1".into()])
        .await;
    // mpv crossed the gapless boundary; the 500 ms tick has not caught up.
    td.fake_mpv.set_playlist_pos(1).await;

    td.core.resync_gapless_preload().await;

    assert!(
        named(&td.fake_mpv.commands().await, "playlist-remove").is_empty(),
        "entry 1 is the playing track and must stay"
    );
}

#[tokio::test]
#[serial]
async fn repeat_change_never_removes_the_entry_mpv_already_plays() {
    let td = TestDaemon::new().await;
    playing_at(&td, 3, 0, RepeatMode::Off).await;
    td.fake_mpv
        .set_playlist(vec!["t-0".into(), "t-1".into()])
        .await;
    td.fake_mpv.set_playlist_pos(1).await;

    td.core.set_repeat_mode(RepeatMode::All).await.unwrap();

    assert!(
        named(&td.fake_mpv.commands().await, "playlist-remove").is_empty(),
        "entry 1 is the playing track and must stay"
    );
}

#[tokio::test]
#[serial]
async fn two_auto_advances_from_one_track_end_move_one_track() {
    let td = TestDaemon::new().await;
    td.fake_subsonic.expect_ping().await;
    playing_at(&td, 4, 0, RepeatMode::Off).await;

    // The end-of-file listener and the idle tick both saw track 0 end.
    td.core.advance_auto_after(Some(0)).await.unwrap();
    td.core.advance_auto_after(Some(0)).await.unwrap();

    assert_eq!(
        td.state.read().await.queue_position,
        Some(1),
        "one track end advances one track, never two"
    );
}

#[tokio::test]
#[serial]
async fn late_end_of_file_after_the_tick_advanced_does_not_advance_again() {
    let td = TestDaemon::new().await;
    td.fake_subsonic.expect_ping().await;
    // The idle tick already moved 0 -> 1 and loaded track 1 (mpv not idle).
    playing_at(&td, 4, 1, RepeatMode::Off).await;
    td.fake_mpv.set_loaded_file("t-1").await;
    let _listener = td.core.spawn_mpv_event_listener().await;

    // Track 0's end-of-file event reaches the listener only now.
    td.fake_mpv.emit_end_file("eof").await;
    tokio::time::sleep(Duration::from_millis(500)).await;

    assert_eq!(
        td.state.read().await.queue_position,
        Some(1),
        "a stale end-of-file must not skip track 1"
    );
}

#[tokio::test]
#[serial]
async fn end_of_file_with_no_preload_advances_one_track() {
    let td = TestDaemon::new().await;
    td.fake_subsonic.expect_ping().await;
    // Track 0 ended with nothing preloaded: mpv is idle.
    playing_at(&td, 4, 0, RepeatMode::Off).await;
    let _listener = td.core.spawn_mpv_event_listener().await;

    td.fake_mpv.emit_end_file("eof").await;
    let moved = {
        let mut ok = false;
        for _ in 0..50 {
            if td.state.read().await.queue_position == Some(1) {
                ok = true;
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        ok
    };
    assert!(moved, "the listener advances when the track truly ended");
}

#[tokio::test]
#[serial]
async fn gapless_change_clears_every_quality_field_of_the_old_track() {
    let td = TestDaemon::new().await;
    playing_at(&td, 3, 0, RepeatMode::Off).await;
    {
        let mut s = td.state.write().await;
        s.now_playing.format = Some("s32".into());
        s.now_playing.channels = Some("stereo".into());
    }
    td.fake_mpv
        .set_playlist(vec!["t-0".into(), "t-1".into()])
        .await;
    td.fake_mpv.set_playlist_pos(1).await;

    td.core.try_gapless_advance_for_test().await;

    let s = td.state.read().await;
    assert_eq!(s.queue_position, Some(1));
    assert_eq!(s.now_playing.format, None, "old sample format cleared");
    assert_eq!(s.now_playing.channels, None, "old channel layout cleared");
}
