//! Stress tests: rapid concurrent operations, deterministic invariants. Included in mutation runs.

mod common;

use common::{song, songs, TestDaemon};
use ferrosonic::daemon::core::PlayMode;
use serde_json::Value;
use serial_test::serial;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[serial]
async fn ten_rapid_album_switches_only_load_one() {
    let td = TestDaemon::new().await;
    for i in 0..10 {
        td.fake_subsonic
            .expect_stream_for(&format!("a{}", i), vec![0u8; 700 * 1024])
            .await;
    }
    {
        let mut s = td.state.write().await;
        s.queue = (0..10)
            .map(|i| song(&format!("a{}", i), &format!("Track {}", i)))
            .collect();
    }
    for i in 0..10 {
        td.core
            .play_queue_position(i, PlayMode::Buffered)
            .await
            .unwrap();
    }
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
    let prebuf_loads = td
        .fake_mpv
        .commands()
        .await
        .iter()
        .filter(|c| {
            c.first().and_then(Value::as_str) == Some("loadfile")
                && c.get(1)
                    .and_then(Value::as_str)
                    .map(|p| p.contains("ferrosonic-prebuf-"))
                    .unwrap_or(false)
        })
        .count();
    assert!(
        prebuf_loads <= 1,
        "10 rapid switches must collapse to <= 1 prebuffer loadfile; saw {}",
        prebuf_loads
    );
    let s = td.state.read().await;
    assert_eq!(s.queue_position, Some(9));
}

#[tokio::test]
#[serial]
async fn concurrent_queue_mutations_preserve_invariants() {
    let td = TestDaemon::new().await;
    {
        let mut s = td.state.write().await;
        s.queue = songs("t", 20);
        s.queue_position = Some(10);
    }
    let core = td.core.clone();
    let state = td.state.clone();
    let h1 = tokio::spawn({
        let core = core.clone();
        async move {
            for _ in 0..50 {
                core.shuffle_queue().await;
            }
        }
    });
    let h2 = tokio::spawn({
        let core = core.clone();
        async move {
            for i in 0..50 {
                core.move_queue_item(i % 20, (i + 5) % 20).await;
            }
        }
    });
    h1.await.unwrap();
    h2.await.unwrap();
    let s = state.read().await;
    assert_eq!(s.queue.len(), 20);
    if let Some(pos) = s.queue_position {
        assert!(pos < s.queue.len(), "queue_position must stay in bounds");
    }
    let mut ids: Vec<_> = s.queue.iter().map(|c| c.id.clone()).collect();
    ids.sort();
    let mut expected: Vec<_> = (0..20).map(|i| format!("t-{}", i)).collect();
    expected.sort();
    assert_eq!(ids, expected, "no songs lost or duplicated");
}

#[tokio::test]
#[serial]
async fn rapid_pause_resume_toggle_doesnt_corrupt_state() {
    use ferrosonic::daemon::state::PlaybackState;
    let td = TestDaemon::new().await;
    {
        let mut s = td.state.write().await;
        s.queue.push(song("a", "A"));
        s.queue_position = Some(0);
        s.now_playing.song = Some(song("a", "A"));
        s.now_playing.state = PlaybackState::Playing;
    }
    for _ in 0..50 {
        td.core.toggle_pause().await.unwrap();
    }
    let s = td.state.read().await;
    assert!(matches!(
        s.now_playing.state,
        PlaybackState::Playing | PlaybackState::Paused
    ));
}

#[tokio::test]
#[serial]
async fn many_set_volume_calls_settle_at_last_value() {
    let td = TestDaemon::new().await;
    for v in 0..=100 {
        td.core.set_volume(v).await.unwrap();
    }
    let saw_100 = td.fake_mpv.commands().await.iter().any(|c| {
        c.first().and_then(Value::as_str) == Some("set_property")
            && c.get(1).and_then(Value::as_str) == Some("volume")
            && c.get(2).and_then(Value::as_f64) == Some(100.0)
    });
    assert!(saw_100, "final set_volume(100) must have reached mpv");
}

#[tokio::test]
#[serial]
async fn enqueue_then_clear_history_idempotent() {
    let td = TestDaemon::new().await;
    {
        let mut s = td.state.write().await;
        s.queue = songs("t", 5);
        s.queue_position = Some(3);
    }
    let n1 = td.core.clear_queue_history().await;
    let n2 = td.core.clear_queue_history().await;
    assert_eq!(n1, 3);
    assert_eq!(n2, 0);
}
