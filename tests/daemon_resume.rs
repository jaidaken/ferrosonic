//! Resume-where-you-left-off: snapshot restore into now-playing state,
//! autoplay-on-start, and idle-exit eligibility for a never-played session.

mod common;

use common::{songs, TestDaemon};
use ferrosonic::daemon::persistence::QueueSnapshot;
use ferrosonic::daemon::state::PlaybackState;
use serde_json::Value;
use serial_test::serial;

fn write_snapshot(position: usize, offset: f64) {
    let mut queue = songs("t", 3);
    if let Some(song) = queue.get_mut(position) {
        song.duration = Some(200);
    }
    QueueSnapshot {
        queue,
        position: Some(position),
        position_secs: Some(offset),
        paused: true,
    }
    .save()
    .expect("save snapshot");
}

#[tokio::test]
#[serial]
async fn restored_snapshot_restores_track_paused_at_offset() {
    let dir = common::tempdir();
    std::env::set_var("FERROSONIC_CONFIG_DIR", dir.path());
    write_snapshot(1, 42.5);

    let td = TestDaemon::new_with_config_dir(dir).await;
    let s = td.state.read().await;
    assert_eq!(
        s.now_playing.state,
        PlaybackState::Paused,
        "a restored session starts paused, never auto-playing"
    );
    assert_eq!(
        s.now_playing.song.as_ref().map(|x| x.id.as_str()),
        Some("t-1")
    );
    assert!((s.now_playing.position - 42.5).abs() < f64::EPSILON);
    assert!((s.now_playing.duration - 200.0).abs() < f64::EPSILON);
}

#[tokio::test]
#[serial]
async fn restored_unplayed_paused_session_is_idle_for_exit() {
    let dir = common::tempdir();
    std::env::set_var("FERROSONIC_CONFIG_DIR", dir.path());
    write_snapshot(0, 5.0);

    let td = TestDaemon::new_with_config_dir(dir).await;
    assert!(
        td.core.is_idle_for_exit().await,
        "a restored, never-played paused session must remain idle-eligible"
    );

    td.core.mark_play_instance_for_test();
    assert!(
        !td.core.is_idle_for_exit().await,
        "once a track has actually played, paused keeps the daemon alive"
    );
}

#[tokio::test]
#[serial]
async fn autoplay_on_start_resumes_the_restored_track() {
    let td = TestDaemon::new().await;
    td.fake_subsonic.expect_ping().await;
    {
        let mut s = td.state.write().await;
        s.queue = songs("t", 2);
        s.queue_position = Some(0);
        s.now_playing.song = Some(s.queue[0].clone());
        s.now_playing.state = PlaybackState::Paused;
        s.now_playing.position = 33.0;
        s.config.autoplay_on_start = true;
    }

    td.core.autoplay_restored_if_configured().await;
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let cmds = td.fake_mpv.commands().await;
    let saw_start = cmds.iter().any(|c| {
        c.first().and_then(Value::as_str) == Some("loadfile")
            && c.get(4).and_then(Value::as_str) == Some("start=33")
    });
    assert!(
        saw_start,
        "autoplay must load the restored track at its saved offset; {cmds:?}"
    );
    assert_eq!(
        td.state.read().await.now_playing.state,
        PlaybackState::Playing
    );
}

#[tokio::test]
#[serial]
async fn autoplay_off_leaves_the_restored_track_paused() {
    let td = TestDaemon::new().await;
    {
        let mut s = td.state.write().await;
        s.queue = songs("t", 2);
        s.queue_position = Some(0);
        s.now_playing.song = Some(s.queue[0].clone());
        s.now_playing.state = PlaybackState::Paused;
        s.now_playing.position = 10.0;
        s.config.autoplay_on_start = false;
    }

    td.core.autoplay_restored_if_configured().await;

    assert_eq!(
        td.state.read().await.now_playing.state,
        PlaybackState::Paused
    );
    assert!(
        !td.fake_mpv
            .commands()
            .await
            .iter()
            .any(|c| c.first().and_then(Value::as_str) == Some("loadfile")),
        "autoplay off must not load anything"
    );
}
