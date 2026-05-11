//! ipc/server.rs: Response/Event/Unknown* frames from client paths.

mod common;

use common::TestDaemon;
use ferrosonic::ipc::frame::{write_frame, Frame};
use ferrosonic::ipc::protocol::{DaemonEvent, DaemonRequest};
use ferrosonic::ipc::server::serve;
use serial_test::serial;
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::net::UnixStream;

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
async fn client_sending_response_frame_is_ignored() {
    let td = TestDaemon::new().await;
    let socket = td.config_dir.path().join("misbehave-resp.sock");
    let core = td.core.clone();
    let socket_path = socket.clone();
    let server = tokio::spawn(async move { serve(core, &socket_path).await });
    assert!(wait_for_socket(&socket, 1500).await);

    let mut stream = UnixStream::connect(&socket).await.unwrap();
    let bad_frame = Frame::Response {
        id: 1,
        payload: Ok(ferrosonic::ipc::DaemonResponse::Pong),
    };
    write_frame(&mut stream, &bad_frame).await.unwrap();
    let ping = Frame::Request {
        id: 2,
        req: DaemonRequest::Ping,
    };
    write_frame(&mut stream, &ping).await.unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    drop(stream);
    server.abort();
}

#[tokio::test]
#[serial]
async fn client_sending_event_frame_is_ignored() {
    let td = TestDaemon::new().await;
    let socket = td.config_dir.path().join("misbehave-evt.sock");
    let core = td.core.clone();
    let socket_path = socket.clone();
    let server = tokio::spawn(async move { serve(core, &socket_path).await });
    assert!(wait_for_socket(&socket, 1500).await);

    let mut stream = UnixStream::connect(&socket).await.unwrap();
    let bad = Frame::Event(DaemonEvent::Notification {
        message: "fake".into(),
        is_error: false,
    });
    write_frame(&mut stream, &bad).await.unwrap();
    let ping = Frame::Request {
        id: 1,
        req: DaemonRequest::Ping,
    };
    write_frame(&mut stream, &ping).await.unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    drop(stream);
    server.abort();
}

#[tokio::test]
#[serial]
async fn client_sending_unknown_request_variant_gets_err_reply() {
    let td = TestDaemon::new().await;
    let socket = td.config_dir.path().join("misbehave-unk.sock");
    let core = td.core.clone();
    let socket_path = socket.clone();
    let server = tokio::spawn(async move { serve(core, &socket_path).await });
    assert!(wait_for_socket(&socket, 1500).await);

    let mut stream = UnixStream::connect(&socket).await.unwrap();
    let unknown_body = br#"{"id":1,"req":{"WhateverDoesNotExist":{}}}"#;
    let len = unknown_body.len() as u32;
    stream.write_all(&len.to_be_bytes()).await.unwrap();
    stream.write_all(unknown_body).await.unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    drop(stream);
    server.abort();
}

#[tokio::test]
#[serial]
async fn server_handles_truncated_frame_gracefully() {
    let td = TestDaemon::new().await;
    let socket = td.config_dir.path().join("misbehave-trunc.sock");
    let core = td.core.clone();
    let socket_path = socket.clone();
    let server = tokio::spawn(async move { serve(core, &socket_path).await });
    assert!(wait_for_socket(&socket, 1500).await);

    let mut stream = UnixStream::connect(&socket).await.unwrap();
    stream.write_all(&[0x00, 0x00, 0x10, 0x00]).await.unwrap();
    stream.write_all(b"truncated").await.unwrap();
    drop(stream);
    tokio::time::sleep(Duration::from_millis(100)).await;
    server.abort();
}
