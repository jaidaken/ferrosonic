//! Buffered playback path: stream bytes to local temp, then loadfile.
//! 0.4.0 album-switch fix relies on this; cancel-flag race covered here.

mod common;

use common::{song, TestDaemon};
use ferrosonic::daemon::core::PlayMode;
use serde_json::Value;
use serial_test::serial;

fn payload(size: usize) -> Vec<u8> {
    vec![0u8; size]
}

#[tokio::test]
#[serial]
async fn buffered_above_threshold_loads_local_temp_file() {
    let td = TestDaemon::new().await;
    td.fake_subsonic
        .expect_stream_for("abc", payload(700 * 1024))
        .await;

    {
        let mut s = td.state.write().await;
        s.queue.push(song("abc", "Track A"));
    }

    td.core
        .play_queue_position(0, PlayMode::Buffered)
        .await
        .unwrap();

    let loaded = td
        .fake_mpv
        .wait_for(5000, |cmds| {
            cmds.iter().any(|c| {
                c.first().and_then(Value::as_str) == Some("loadfile")
                    && c.get(1)
                        .and_then(Value::as_str)
                        .map(|p| p.starts_with("/tmp/") || p.contains("ferrosonic-prebuf-"))
                        .unwrap_or(false)
            })
        })
        .await;
    assert!(
        loaded,
        "Buffered mode must loadfile with a local temp path; commands: {:?}",
        td.fake_mpv.commands().await
    );
}

#[tokio::test]
#[serial]
async fn buffered_below_threshold_still_loads_on_eof() {
    let td = TestDaemon::new().await;
    td.fake_subsonic
        .expect_stream_for("small", payload(100 * 1024))
        .await;

    {
        let mut s = td.state.write().await;
        s.queue.push(song("small", "Small Track"));
    }

    td.core
        .play_queue_position(0, PlayMode::Buffered)
        .await
        .unwrap();

    let loaded = td
        .fake_mpv
        .wait_for(5000, |cmds| {
            cmds.iter()
                .any(|c| c.first().and_then(Value::as_str) == Some("loadfile"))
        })
        .await;
    assert!(
        loaded,
        "small file should still trigger loadfile via the EOF path"
    );
}

#[tokio::test]
#[serial]
async fn buffered_play_stops_previous_audio_immediately() {
    let td = TestDaemon::new().await;
    td.fake_subsonic
        .expect_stream_for("abc", payload(700 * 1024))
        .await;

    {
        let mut s = td.state.write().await;
        s.queue.push(song("abc", "Track"));
        s.now_playing.state = ferrosonic::app::state::PlaybackState::Playing;
    }

    td.fake_mpv
        .set_loaded_file("http://previous-track.mp3")
        .await;

    td.core
        .play_queue_position(0, PlayMode::Buffered)
        .await
        .unwrap();

    let saw_stop = td
        .fake_mpv
        .wait_for(2000, |cmds| {
            cmds.iter()
                .any(|c| c.first().and_then(Value::as_str) == Some("stop"))
        })
        .await;
    assert!(
        saw_stop,
        "Buffered mode must call mpv stop before the new prebuffer"
    );
}

#[tokio::test]
#[serial]
async fn rapid_buffered_switches_only_load_latest_track() {
    let td = TestDaemon::new().await;
    td.fake_subsonic
        .expect_stream_for("a", payload(700 * 1024))
        .await;
    td.fake_subsonic
        .expect_stream_for("b", payload(700 * 1024))
        .await;
    td.fake_subsonic
        .expect_stream_for("c", payload(700 * 1024))
        .await;

    {
        let mut s = td.state.write().await;
        s.queue.push(song("a", "A"));
        s.queue.push(song("b", "B"));
        s.queue.push(song("c", "C"));
    }

    td.core
        .play_queue_position(0, PlayMode::Buffered)
        .await
        .unwrap();
    td.core
        .play_queue_position(1, PlayMode::Buffered)
        .await
        .unwrap();
    td.core
        .play_queue_position(2, PlayMode::Buffered)
        .await
        .unwrap();

    let _ = td
        .fake_mpv
        .wait_for(5000, |cmds| {
            cmds.iter().any(|c| {
                c.first().and_then(Value::as_str) == Some("loadfile")
                    && c.get(1)
                        .and_then(Value::as_str)
                        .map(|p| p.contains("ferrosonic-prebuf-"))
                        .unwrap_or(false)
            })
        })
        .await;

    let loadfiles: Vec<String> = td
        .fake_mpv
        .commands()
        .await
        .iter()
        .filter(|c| c.first().and_then(Value::as_str) == Some("loadfile"))
        .filter_map(|c| c.get(1).and_then(Value::as_str).map(String::from))
        .collect();

    let prebuffered: Vec<_> = loadfiles
        .iter()
        .filter(|p| p.contains("ferrosonic-prebuf-"))
        .collect();
    assert!(
        prebuffered.len() <= 1,
        "rapid switches must cancel previous prebuffers; saw {} prebuffered loadfiles: {:?}",
        prebuffered.len(),
        prebuffered
    );

    let s = td.state.read().await;
    assert_eq!(
        s.queue_position,
        Some(2),
        "queue_position points at most recent switch"
    );
}
