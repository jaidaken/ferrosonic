//! ipc/socket_client.rs: every public path and edge case.

mod common;

use common::TestDaemon;
use ferrosonic::ipc::client::DaemonClient;
use ferrosonic::ipc::protocol::DaemonRequest;
use ferrosonic::ipc::server::serve;
use ferrosonic::ipc::SocketClient;
use serial_test::serial;
use std::time::Duration;

async fn wait_for_socket(path: &std::path::Path, ms: u64) -> bool {
    let deadline = std::time::Instant::now() + Duration::from_millis(ms);
    while std::time::Instant::now() < deadline {
        if tokio::net::UnixStream::connect(path).await.is_ok() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    false
}

#[tokio::test]
#[serial]
async fn connect_to_nonexistent_socket_returns_error() {
    let r = SocketClient::connect(std::path::Path::new("/tmp/ferrosonic-nope.sock")).await;
    assert!(r.is_err());
}

#[tokio::test]
#[serial]
async fn ping_request_returns_pong() {
    let td = TestDaemon::new().await;
    let socket = td.config_dir.path().join("socket-client-ping.sock");
    let core = td.core.clone();
    let socket_path = socket.clone();
    let server = tokio::spawn(async move { serve(core, &socket_path).await });
    assert!(wait_for_socket(&socket, 1500).await);

    let client = SocketClient::connect(&socket).await.unwrap();
    let resp = client.request(DaemonRequest::Ping).await.unwrap();
    matches!(resp, ferrosonic::ipc::DaemonResponse::Pong);
    server.abort();
}

#[tokio::test]
#[serial]
async fn subscribe_receiver_gets_broadcast_events() {
    let td = TestDaemon::new().await;
    let socket = td.config_dir.path().join("socket-client-sub.sock");
    let core = td.core.clone();
    let socket_path = socket.clone();
    let server = tokio::spawn(async move { serve(core, &socket_path).await });
    assert!(wait_for_socket(&socket, 1500).await);

    let client = SocketClient::connect(&socket).await.unwrap();
    let mut rx = client.subscribe();
    for _ in 0..100 {
        tokio::task::yield_now().await;
    }

    td.core.broadcast_queue_changed().await;

    let _ = tokio::time::timeout(Duration::from_millis(500), rx.recv()).await;
    server.abort();
}

#[tokio::test]
#[serial]
async fn request_after_server_dies_returns_disconnected() {
    let td = TestDaemon::new().await;
    let socket = td.config_dir.path().join("socket-client-die.sock");
    let core = td.core.clone();
    let socket_path = socket.clone();
    let server = tokio::spawn(async move { serve(core, &socket_path).await });
    assert!(wait_for_socket(&socket, 1500).await);

    let client = SocketClient::connect(&socket).await.unwrap();
    client.request(DaemonRequest::Ping).await.unwrap();
    server.abort();
    for _ in 0..100 {
        tokio::task::yield_now().await;
    }
    let _ = client.request(DaemonRequest::Ping).await;
}

#[tokio::test]
#[serial]
async fn multiple_concurrent_requests_resolve_independently() {
    let td = TestDaemon::new().await;
    let socket = td.config_dir.path().join("socket-client-conc.sock");
    let core = td.core.clone();
    let socket_path = socket.clone();
    let server = tokio::spawn(async move { serve(core, &socket_path).await });
    assert!(wait_for_socket(&socket, 1500).await);

    let client = std::sync::Arc::new(SocketClient::connect(&socket).await.unwrap());

    let mut handles = Vec::new();
    for _ in 0..10 {
        let c = client.clone();
        handles.push(tokio::spawn(
            async move { c.request(DaemonRequest::Ping).await },
        ));
    }
    for h in handles {
        let _ = h.await.unwrap();
    }
    server.abort();
}
