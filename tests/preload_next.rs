//! `preload_next_track` gapless: appends the next track to mpv playlist.

mod common;

use common::{songs, TestDaemon};
use ferrosonic::config::RepeatMode;
use serde_json::Value;
use serial_test::serial;

#[tokio::test]
#[serial]
async fn preload_appends_next_song_to_mpv_playlist() {
    let td = TestDaemon::new().await;
    {
        let mut s = td.state.write().await;
        s.queue = songs("t", 3);
        s.queue_position = Some(0);
    }

    td.core.preload_next_track(0).await;

    let saw_append = td.fake_mpv.commands().await.iter().any(|c| {
        c.first().and_then(Value::as_str) == Some("loadfile")
            && c.get(2).and_then(Value::as_str) == Some("append")
    });
    assert!(
        saw_append,
        "preload_next_track must loadfile with append mode"
    );
}

#[tokio::test]
#[serial]
async fn preload_at_end_of_queue_with_repeat_off_does_nothing() {
    let td = TestDaemon::new().await;
    {
        let mut s = td.state.write().await;
        s.queue = songs("t", 3);
        s.queue_position = Some(2);
        s.config.repeat_mode = RepeatMode::Off;
    }

    td.core.preload_next_track(2).await;

    let saw_append = td.fake_mpv.commands().await.iter().any(|c| {
        c.first().and_then(Value::as_str) == Some("loadfile")
            && c.get(2).and_then(Value::as_str) == Some("append")
    });
    assert!(
        !saw_append,
        "Off + last track: nothing to preload, no append should fire"
    );
}

#[tokio::test]
#[serial]
async fn preload_at_end_under_repeat_all_loads_first_track() {
    let td = TestDaemon::new().await;
    {
        let mut s = td.state.write().await;
        s.queue = songs("t", 3);
        s.queue_position = Some(2);
        s.config.repeat_mode = RepeatMode::All;
    }

    td.core.preload_next_track(2).await;

    let appends: Vec<String> = td
        .fake_mpv
        .commands()
        .await
        .iter()
        .filter(|c| {
            c.first().and_then(Value::as_str) == Some("loadfile")
                && c.get(2).and_then(Value::as_str) == Some("append")
        })
        .filter_map(|c| c.get(1).and_then(Value::as_str).map(String::from))
        .collect();

    assert_eq!(appends.len(), 1, "repeat-All wraps; expected one append");
    assert!(
        appends[0].contains("id=t-0"),
        "wrap target should be queue[0]; got {}",
        appends[0]
    );
}

#[tokio::test]
#[serial]
async fn preload_under_repeat_one_preloads_same_track() {
    let td = TestDaemon::new().await;
    {
        let mut s = td.state.write().await;
        s.queue = songs("t", 3);
        s.queue_position = Some(1);
        s.config.repeat_mode = RepeatMode::One;
    }

    td.core.preload_next_track(1).await;

    let appends: Vec<String> = td
        .fake_mpv
        .commands()
        .await
        .iter()
        .filter(|c| {
            c.first().and_then(Value::as_str) == Some("loadfile")
                && c.get(2).and_then(Value::as_str) == Some("append")
        })
        .filter_map(|c| c.get(1).and_then(Value::as_str).map(String::from))
        .collect();

    assert_eq!(appends.len(), 1);
    assert!(
        appends[0].contains("id=t-1"),
        "repeat-One preloads the same track; got {}",
        appends[0]
    );
}

#[tokio::test]
#[serial]
async fn preload_without_subsonic_client_is_safe() {
    let td = TestDaemon::new().await;
    {
        let mut sub = td.core.subsonic.write().await;
        *sub = None;
    }
    {
        let mut s = td.state.write().await;
        s.queue = songs("t", 2);
        s.queue_position = Some(0);
    }

    td.core.preload_next_track(0).await;

    let saw_append = td.fake_mpv.commands().await.iter().any(|c| {
        c.first().and_then(Value::as_str) == Some("loadfile")
            && c.get(2).and_then(Value::as_str) == Some("append")
    });
    assert!(!saw_append, "no subsonic client: no preload");
}
