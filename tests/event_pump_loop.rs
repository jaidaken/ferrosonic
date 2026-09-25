//! Daemon-mode event pump: events reach the TUI state, a lagged stream is
//! repaired from a fresh daemon snapshot, and a closed stream ends the pump.

mod common;

use std::sync::Arc;
use std::time::Duration;

use common::{render::empty_cover_art_state, songs, TestDaemon};
use ferrosonic::app::event_pump::run_event_pump;
use ferrosonic::app::state::{new_shared_client_state, new_shared_daemon_state, SharedDaemonState};
use ferrosonic::config::Config;
use ferrosonic::ipc::client::DaemonClient;
use ferrosonic::ipc::{DaemonEvent, InProcessClient};
use serial_test::serial;
use tokio::sync::broadcast;

async fn wait_for_queue_len(ds: &SharedDaemonState, want: usize) -> usize {
    for _ in 0..200 {
        let len = ds.read().await.queue.len();
        if len == want {
            return len;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    ds.read().await.queue.len()
}

#[tokio::test]
#[serial]
async fn queue_event_reaches_the_tui_state_and_a_closed_stream_ends_the_pump() {
    let td = TestDaemon::new().await;
    let client: Arc<dyn DaemonClient> = Arc::new(InProcessClient::new(td.core.clone()));
    let ds = new_shared_daemon_state(Config::new());
    let (tx, rx) = broadcast::channel(8);
    let pump = tokio::spawn(run_event_pump(
        client,
        ds.clone(),
        new_shared_client_state(&Config::new()),
        empty_cover_art_state(),
        rx,
    ));

    tx.send(DaemonEvent::QueueChanged {
        queue: songs("q", 2),
        position: Some(1),
    })
    .expect("the pump subscribes");

    assert_eq!(wait_for_queue_len(&ds, 2).await, 2);
    assert_eq!(ds.read().await.queue_position, Some(1));
    drop(tx);
    tokio::time::timeout(Duration::from_secs(5), pump)
        .await
        .expect("the pump ends when the stream closes")
        .expect("pump task joins");
}

#[tokio::test]
#[serial]
async fn lagged_stream_is_repaired_from_a_daemon_snapshot() {
    let td = TestDaemon::new().await;
    {
        let mut s = td.state.write().await;
        s.queue = songs("d", 3);
        s.queue_position = Some(2);
    }
    let client: Arc<dyn DaemonClient> = Arc::new(InProcessClient::new(td.core.clone()));
    let ds = new_shared_daemon_state(Config::new());
    let (tx, rx) = broadcast::channel(4);
    let pump = tokio::spawn(run_event_pump(
        client,
        ds.clone(),
        new_shared_client_state(&Config::new()),
        empty_cover_art_state(),
        rx,
    ));

    // The pump has not run yet on this single-thread runtime, so 10 events
    // overflow the 4-slot channel; none of them carries the queue.
    for i in 0..10 {
        tx.send(DaemonEvent::Notification {
            message: format!("n{i}"),
            is_error: false,
        })
        .expect("the pump subscribes");
    }

    assert_eq!(
        wait_for_queue_len(&ds, 3).await,
        3,
        "the missed state comes back from a daemon snapshot"
    );
    assert_eq!(ds.read().await.queue_position, Some(2));
    pump.abort();
}
