//! set_repeat_mode drops the stale preloaded next (mpv playlist slot 1) only
//! when the playlist actually has one (count > 1). Kills the `count > 1`
//! boundary mutants in settings_ops; the playlist-remove command is observable.

mod common;

use common::{songs, TestDaemon};
use ferrosonic::config::RepeatMode;
use serde_json::Value;
use serial_test::serial;

fn saw_playlist_remove(cmds: &[Vec<Value>]) -> bool {
    cmds.iter()
        .any(|c| c.first().and_then(Value::as_str) == Some("playlist-remove"))
}

#[tokio::test]
#[serial]
async fn repeat_mode_change_trims_preload_when_a_next_is_loaded() {
    let td = TestDaemon::new().await;
    td.fake_mpv
        .set_playlist(vec!["current".into(), "preloaded".into()])
        .await;
    {
        let mut s = td.state.write().await;
        s.queue = songs("q", 3);
        s.queue_position = Some(0);
    }

    td.core.set_repeat_mode(RepeatMode::One).await.unwrap();

    assert!(
        saw_playlist_remove(&td.fake_mpv.commands().await),
        "with a preloaded next (playlist count 2) the stale slot is removed"
    );
}

#[tokio::test]
#[serial]
async fn repeat_mode_change_does_not_trim_when_no_preload() {
    let td = TestDaemon::new().await;
    td.fake_mpv.set_playlist(vec!["current".into()]).await;
    {
        let mut s = td.state.write().await;
        s.queue = songs("q", 3);
        s.queue_position = Some(0);
    }

    td.core.set_repeat_mode(RepeatMode::All).await.unwrap();

    assert!(
        !saw_playlist_remove(&td.fake_mpv.commands().await),
        "with no preloaded next (count 1) nothing is removed"
    );
}
