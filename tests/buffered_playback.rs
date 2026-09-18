//! Playback handoff strategy: streaming vs buffered.
//!
//! `StreamOnStart` (default on) makes a cold queue start load the authenticated
//! `rest/stream` URL so mpv begins as soon as it has bytes; with it off, the
//! whole track is buffered to a local temp file first. The 0.4.0 album-switch
//! fix relies on the buffered path; the cancel-flag race is covered here too.

mod common;

use common::{song, TestDaemon};
use ferrosonic::daemon::core::PlayMode;
use ferrosonic::ipc::client::{DaemonClient, InProcessClient};
use ferrosonic::ipc::protocol::{DaemonRequest, EnqueueMode};
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
        s.now_playing.state = ferrosonic::daemon::state::PlaybackState::Playing;
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

#[tokio::test]
#[serial]
async fn direct_play_cancels_an_inflight_prebuffer() {
    let td = TestDaemon::new().await;
    // Track A's stream is withheld for 1.5s so its Buffered download is still
    // in flight when the Direct load of B supersedes it.
    td.fake_subsonic
        .expect_stream_for_delayed("a", payload(2 * 1024 * 1024), 1500)
        .await;
    td.fake_subsonic
        .expect_stream_for("b", payload(64 * 1024))
        .await;

    {
        let mut s = td.state.write().await;
        s.queue.push(song("a", "A"));
        s.queue.push(song("b", "B"));
    }

    td.core
        .play_queue_position(0, PlayMode::Buffered)
        .await
        .unwrap();
    td.core
        .play_queue_position(1, PlayMode::Direct)
        .await
        .unwrap();

    // Give the abandoned pre-buffer download time to finish so a missing
    // cancellation would surface as a late local-file loadfile that clobbers B.
    tokio::time::sleep(std::time::Duration::from_secs(3)).await;

    let loadfiles: Vec<String> = td
        .fake_mpv
        .commands()
        .await
        .iter()
        .filter(|c| c.first().and_then(Value::as_str) == Some("loadfile"))
        .filter_map(|c| c.get(1).and_then(Value::as_str).map(String::from))
        .collect();
    assert!(
        !loadfiles.iter().any(|p| p.contains("ferrosonic-prebuf-")),
        "a superseded pre-buffer must not loadfile after a Direct load: {loadfiles:?}"
    );

    let s = td.state.read().await;
    assert_eq!(
        s.now_playing.song.as_ref().map(|x| x.id.as_str()),
        Some("b"),
        "the Directly-loaded track remains current"
    );
    assert_eq!(s.queue_position, Some(1));
}

#[derive(Clone, Copy)]
enum SupersedingAction {
    Direct,
    Pause,
    Stop,
    Halt,
    QueueEnd,
}

async fn assert_forced_fallback_is_cancelled(kind: u8, action: SupersedingAction) {
    let td = TestDaemon::new().await;
    td.fake_subsonic
        .expect_stream_for("a", payload(64 * 1024))
        .await;
    {
        let mut s = td.state.write().await;
        s.queue = vec![song("a", "A"), song("b", "B")];
    }
    td.core.force_prebuffer_failure_for_test(kind);
    td.core
        .play_queue_position(0, PlayMode::Buffered)
        .await
        .unwrap();
    td.core.wait_prebuffer_failure_for_test().await;

    match action {
        SupersedingAction::Direct => td
            .core
            .play_queue_position(1, PlayMode::Direct)
            .await
            .unwrap(),
        SupersedingAction::Pause => td.core.pause_playback().await.unwrap(),
        SupersedingAction::Stop => td.core.stop_playback().await.unwrap(),
        SupersedingAction::Halt => td.core.halt_keep_queue().await,
        SupersedingAction::QueueEnd => {
            td.state.write().await.queue.truncate(1);
            td.core.next_track().await.unwrap();
        }
    }
    td.core.release_prebuffer_failure_for_test();
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let stream_a = format!("{}/rest/stream", td.fake_subsonic.url());
    let loads = td
        .fake_mpv
        .commands()
        .await
        .into_iter()
        .filter(|c| c.first().and_then(Value::as_str) == Some("loadfile"))
        .filter_map(|c| c.get(1).and_then(Value::as_str).map(str::to_owned))
        .collect::<Vec<_>>();
    assert!(
        !loads
            .iter()
            .any(|url| url.starts_with(&stream_a) && url.contains("id=a")),
        "a cancelled fallback loaded obsolete track A: {loads:?}"
    );
}

#[tokio::test]
#[serial]
async fn cancelled_network_fallback_cannot_override_direct_play() {
    assert_forced_fallback_is_cancelled(1, SupersedingAction::Direct).await;
}

#[tokio::test]
#[serial]
async fn cancelled_disk_fallback_cannot_override_pause_stop_halt_or_queue_end() {
    for action in [
        SupersedingAction::Pause,
        SupersedingAction::Stop,
        SupersedingAction::Halt,
        SupersedingAction::QueueEnd,
    ] {
        assert_forced_fallback_is_cancelled(2, action).await;
    }
}

/// `StreamOnStart` defaults on: a cold queue replacement must hand mpv the
/// authenticated stream URL instead of downloading the whole track to a temp
/// file first, so playback can begin as soon as mpv has bytes.
#[tokio::test]
#[serial]
async fn enqueue_replace_streams_when_stream_on_start_defaults_on() {
    let td = TestDaemon::new().await;
    assert!(
        td.state.read().await.config.stream_on_start,
        "StreamOnStart must default on"
    );
    td.fake_subsonic.expect_ping().await;
    let client = InProcessClient::new(td.core.clone());

    client
        .request(DaemonRequest::EnqueueSongs {
            songs: vec![song("abc", "Track A")],
            mode: EnqueueMode::Replace { play_from: Some(0) },
        })
        .await
        .unwrap();

    let stream_prefix = format!("{}/rest/stream", td.fake_subsonic.url());
    let loaded = td
        .fake_mpv
        .wait_for(5000, |cmds| {
            cmds.iter().any(|c| {
                c.first().and_then(Value::as_str) == Some("loadfile")
                    && c.get(1)
                        .and_then(Value::as_str)
                        .is_some_and(|p| p.starts_with(&stream_prefix) && p.contains("id=abc"))
            })
        })
        .await;
    assert!(
        loaded,
        "stream-on-start must loadfile the authenticated stream URL; commands: {:?}",
        td.fake_mpv.commands().await
    );

    let loads: Vec<String> = td
        .fake_mpv
        .commands()
        .await
        .iter()
        .filter(|c| c.first().and_then(Value::as_str) == Some("loadfile"))
        .filter_map(|c| c.get(1).and_then(Value::as_str).map(String::from))
        .collect();
    assert!(
        !loads.iter().any(|p| p.contains("ferrosonic-prebuf-")),
        "stream-on-start must not pre-buffer to a temp file: {loads:?}"
    );
}

/// With `StreamOnStart` off, the same queue replacement keeps the pre-0.4.x
/// behavior: download the whole track to a local temp file, then load it.
#[tokio::test]
#[serial]
async fn enqueue_replace_buffers_when_stream_on_start_disabled() {
    let td = TestDaemon::new().await;
    td.state.write().await.config.stream_on_start = false;
    td.fake_subsonic
        .expect_stream_for("abc", payload(700 * 1024))
        .await;
    let client = InProcessClient::new(td.core.clone());

    client
        .request(DaemonRequest::EnqueueSongs {
            songs: vec![song("abc", "Track A")],
            mode: EnqueueMode::Replace { play_from: Some(0) },
        })
        .await
        .unwrap();

    let loaded = td
        .fake_mpv
        .wait_for(5000, |cmds| {
            cmds.iter().any(|c| {
                c.first().and_then(Value::as_str) == Some("loadfile")
                    && c.get(1)
                        .and_then(Value::as_str)
                        .is_some_and(|p| p.contains("ferrosonic-prebuf-"))
            })
        })
        .await;
    assert!(
        loaded,
        "stream-on-start off must pre-buffer to a temp file; commands: {:?}",
        td.fake_mpv.commands().await
    );
}
