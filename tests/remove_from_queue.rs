//! `DaemonRequest::RemoveFromQueue` index math + playback continuity.
//! Regression for ng#118 (removing the currently-playing song must stop or advance, not orphan).

mod common;

use common::{song, songs, TestDaemon};
use ferrosonic::ipc::client::{DaemonClient, InProcessClient};
use ferrosonic::ipc::protocol::DaemonRequest;
use serde_json::Value;
use serial_test::serial;

#[tokio::test]
#[serial]
async fn remove_before_current_decrements_position() {
    let td = TestDaemon::new().await;
    let client = InProcessClient::new(td.core.clone());
    {
        let mut s = td.state.write().await;
        s.queue = songs("t", 4);
        s.queue_position = Some(2);
    }

    client
        .request(DaemonRequest::RemoveFromQueue(0))
        .await
        .unwrap();

    let s = td.state.read().await;
    assert_eq!(
        s.queue.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(),
        vec!["t-1", "t-2", "t-3"]
    );
    assert_eq!(s.queue_position, Some(1));
}

#[tokio::test]
#[serial]
async fn remove_after_current_keeps_position() {
    let td = TestDaemon::new().await;
    let client = InProcessClient::new(td.core.clone());
    {
        let mut s = td.state.write().await;
        s.queue = songs("t", 4);
        s.queue_position = Some(1);
    }

    client
        .request(DaemonRequest::RemoveFromQueue(3))
        .await
        .unwrap();

    let s = td.state.read().await;
    assert_eq!(s.queue.len(), 3);
    assert_eq!(s.queue_position, Some(1));
}

#[tokio::test]
#[serial]
async fn remove_current_with_successor_starts_next_track() {
    let td = TestDaemon::new().await;
    td.fake_subsonic.expect_ping().await;
    let client = InProcessClient::new(td.core.clone());
    {
        let mut s = td.state.write().await;
        s.queue = songs("t", 3);
        s.queue_position = Some(1);
    }

    client
        .request(DaemonRequest::RemoveFromQueue(1))
        .await
        .unwrap();

    let saw_loadfile = td
        .fake_mpv
        .wait_for(2000, |cmds| {
            cmds.iter()
                .any(|c| c.first().and_then(Value::as_str) == Some("loadfile"))
        })
        .await;
    assert!(
        saw_loadfile,
        "removing currently-playing track must start the successor via loadfile"
    );
}

#[tokio::test]
#[serial]
async fn remove_current_at_end_halts_playback() {
    let td = TestDaemon::new().await;
    let client = InProcessClient::new(td.core.clone());
    {
        let mut s = td.state.write().await;
        s.queue = songs("t", 3);
        s.queue_position = Some(2);
    }

    client
        .request(DaemonRequest::RemoveFromQueue(2))
        .await
        .unwrap();

    let s = td.state.read().await;
    assert_eq!(s.queue.len(), 2);
    assert_eq!(
        s.queue_position, None,
        "removing the last track when it was current must clear queue_position"
    );
}

#[tokio::test]
#[serial]
async fn remove_past_end_is_noop() {
    let td = TestDaemon::new().await;
    let client = InProcessClient::new(td.core.clone());
    {
        let mut s = td.state.write().await;
        s.queue = vec![song("a", "A"), song("b", "B")];
        s.queue_position = Some(0);
    }

    client
        .request(DaemonRequest::RemoveFromQueue(99))
        .await
        .unwrap();

    let s = td.state.read().await;
    assert_eq!(s.queue.len(), 2);
    assert_eq!(s.queue_position, Some(0));
}
