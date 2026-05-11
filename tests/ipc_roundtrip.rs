//! End-to-end IPC: socket server + socket client round-trip.

mod common;

use common::TestDaemon;
use ferrosonic::ipc::client::DaemonClient;
use ferrosonic::ipc::protocol::{DaemonRequest, DaemonResponse};
use ferrosonic::ipc::server::serve;
use ferrosonic::ipc::SocketClient;
use serial_test::serial;

#[tokio::test]
#[serial]
async fn ping_round_trips_over_unix_socket() {
    let td = TestDaemon::new().await;
    let socket = td.config_dir.path().join("test.sock");

    let core = td.core.clone();
    let socket_path = socket.clone();
    let server_task = tokio::spawn(async move {
        let _ = serve(core, &socket_path).await;
    });

    for _ in 0..30 {
        if socket.exists() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    assert!(socket.exists(), "server should bind the socket");

    let client = SocketClient::connect(&socket).await.expect("connect");

    let resp = client
        .request(DaemonRequest::Ping)
        .await
        .expect("Ping round-trip");
    assert!(
        matches!(resp, DaemonResponse::Pong),
        "Ping must return Pong, got {:?}",
        resp
    );

    server_task.abort();
}

#[tokio::test]
#[serial]
async fn shuffle_queue_request_routes_through_socket() {
    let td = TestDaemon::new().await;
    let socket = td.config_dir.path().join("test.sock");

    {
        let mut s = td.state.write().await;
        s.queue = common::songs("t", 5);
        s.queue_position = Some(0);
    }

    let core = td.core.clone();
    let socket_path = socket.clone();
    let server_task = tokio::spawn(async move {
        let _ = serve(core, &socket_path).await;
    });
    for _ in 0..30 {
        if socket.exists() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }

    let client = SocketClient::connect(&socket).await.unwrap();
    let resp = client.request(DaemonRequest::ShuffleQueue).await.unwrap();
    assert!(matches!(resp, DaemonResponse::Ok));

    let s = td.state.read().await;
    assert_eq!(s.queue.len(), 5, "shuffle preserves length");
    assert_eq!(
        s.queue_position,
        Some(0),
        "shuffle preserves current position"
    );

    server_task.abort();
}
