//! ipc/server edge cases: stale socket cleanup, mid-stream EOF.

mod common;

use std::sync::Arc;
use std::time::Duration;

use common::TestDaemon;
use ferrosonic::ipc::client::DaemonClient;
use ferrosonic::ipc::protocol::DaemonRequest;
use ferrosonic::ipc::server::serve;
use ferrosonic::ipc::SocketClient;
use serial_test::serial;

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
async fn stale_socket_file_is_replaced_on_bind() {
    let td = TestDaemon::new().await;
    let socket = td.config_dir.path().join("stale.sock");
    std::fs::write(&socket, b"not actually a socket").unwrap();
    assert!(socket.exists());

    let core = td.core.clone();
    let socket_path = socket.clone();
    let server = tokio::spawn(async move { serve(core, &socket_path).await });

    assert!(wait_for_socket(&socket, 1500).await);

    let client = SocketClient::connect(&socket).await.expect("connect");
    client.request(DaemonRequest::Ping).await.expect("ping");

    server.abort();
}

#[tokio::test]
#[serial]
async fn stale_socket_with_running_daemon_returns_error() {
    let td = TestDaemon::new().await;
    let socket = td.config_dir.path().join("contested.sock");

    let core1 = td.core.clone();
    let socket1 = socket.clone();
    let server1 = tokio::spawn(async move { serve(core1, &socket1).await });
    assert!(wait_for_socket(&socket, 1500).await);

    let core2 = td.core.clone();
    let socket2 = socket.clone();
    let result = tokio::time::timeout(
        Duration::from_millis(500),
        tokio::spawn(async move { serve(core2, &socket2).await }),
    )
    .await;

    server1.abort();
    drop(result);
}

#[tokio::test]
#[serial]
async fn client_disconnect_mid_request_does_not_crash_server() {
    let td = TestDaemon::new().await;
    let socket = td.config_dir.path().join("disconnect.sock");

    let core = td.core.clone();
    let socket_path = socket.clone();
    let server = tokio::spawn(async move { serve(core, &socket_path).await });
    assert!(wait_for_socket(&socket, 1500).await);

    {
        let _client = SocketClient::connect(&socket).await.expect("connect");
    }

    let c2 = SocketClient::connect(&socket).await.expect("connect again");
    c2.request(DaemonRequest::Ping)
        .await
        .expect("server still alive");

    server.abort();
}

#[tokio::test]
#[serial]
async fn server_handles_many_concurrent_clients() {
    let td = TestDaemon::new().await;
    let socket = td.config_dir.path().join("concurrent.sock");

    let core = td.core.clone();
    let socket_path = socket.clone();
    let server = tokio::spawn(async move { serve(core, &socket_path).await });
    assert!(wait_for_socket(&socket, 1500).await);

    let mut handles = Vec::new();
    for _ in 0..10 {
        let s = socket.clone();
        handles.push(tokio::spawn(async move {
            let c = SocketClient::connect(&s).await.expect("connect");
            c.request(DaemonRequest::Ping).await.expect("ping")
        }));
    }
    for h in handles {
        let _ = h.await;
    }

    server.abort();
}

#[tokio::test]
#[serial]
async fn server_event_broadcast_reaches_subscribed_client() {
    let td = TestDaemon::new().await;
    let socket = td.config_dir.path().join("events.sock");

    let core = td.core.clone();
    let socket_path = socket.clone();
    let server = tokio::spawn(async move { serve(core, &socket_path).await });
    assert!(wait_for_socket(&socket, 1500).await);

    let client: Arc<SocketClient> = SocketClient::connect(&socket).await.expect("connect");
    let _rx = client.subscribe();
    client.request(DaemonRequest::Ping).await.unwrap();

    server.abort();
}
