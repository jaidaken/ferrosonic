//! mpv controller fed scripted bad responses through a hand-rolled
//! Unix socket server: malformed JSON, request-id mismatch, sudden close.

mod common;
use std::path::PathBuf;
use std::time::Duration;

use ferrosonic::audio::mpv::MpvController;
use tempfile::TempDir;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixListener;
use tokio::sync::oneshot;

enum Behavior {
    MalformedJson,
    WrongRequestId,
    CloseImmediately,
    EmitOnlyEvents,
}

async fn spawn_misbehaving_mpv(behavior: Behavior) -> (TempDir, PathBuf, oneshot::Sender<()>) {
    let tempdir = common::tempdir();
    let socket = tempdir.path().join("bad-mpv.sock");
    let listener = UnixListener::bind(&socket).expect("bind");
    let (shutdown_tx, mut shutdown_rx) = oneshot::channel();

    tokio::spawn(async move {
        tokio::select! {
            _ = &mut shutdown_rx => {}
            accept_res = listener.accept() => {
                if let Ok((stream, _)) = accept_res {
                    let (read_half, mut write_half) = stream.into_split();
                    let mut reader = BufReader::new(read_half);
                    if !matches!(behavior, Behavior::CloseImmediately) {
                        let mut line = String::new();
                        loop {
                            line.clear();
                            let n = match reader.read_line(&mut line).await {
                                Ok(n) => n,
                                Err(_) => break,
                            };
                            if n == 0 {
                                break;
                            }
                            let reply: &[u8] = match behavior {
                                Behavior::MalformedJson => b"this is not json\n",
                                Behavior::WrongRequestId => {
                                    b"{\"request_id\":99999,\"error\":\"success\"}\n"
                                }
                                Behavior::EmitOnlyEvents => {
                                    b"{\"event\":\"property-change\",\"name\":\"pause\",\"data\":true}\n"
                                }
                                Behavior::CloseImmediately => break,
                            };
                            if write_half.write_all(reply).await.is_err() {
                                break;
                            }
                        }
                    }
                }
            }
        }
    });

    (tempdir, socket, shutdown_tx)
}

#[tokio::test]
async fn malformed_json_response_is_ignored_until_timeout() {
    let (_dir, socket, _tx) = spawn_misbehaving_mpv(Behavior::MalformedJson).await;
    let mut mpv = MpvController::with_socket_path(socket);
    mpv.connect_to_existing().await.unwrap();
    let r = tokio::time::timeout(Duration::from_millis(800), mpv.pause()).await;
    assert!(
        r.is_err() || r.as_ref().unwrap().is_err(),
        "malformed JSON must not silently succeed; got {:?}",
        r
    );
}

#[tokio::test]
async fn wrong_request_id_does_not_resolve_command() {
    let (_dir, socket, _tx) = spawn_misbehaving_mpv(Behavior::WrongRequestId).await;
    let mut mpv = MpvController::with_socket_path(socket);
    mpv.connect_to_existing().await.unwrap();
    let r = tokio::time::timeout(Duration::from_millis(600), mpv.pause()).await;
    assert!(
        r.is_err(),
        "request-id mismatch should leave the call waiting; expected timeout"
    );
}

#[tokio::test]
async fn close_immediately_propagates_socket_closed_error() {
    let (_dir, socket, _tx) = spawn_misbehaving_mpv(Behavior::CloseImmediately).await;
    let mut mpv = MpvController::with_socket_path(socket);
    mpv.connect_to_existing().await.unwrap();
    let r = mpv.pause().await;
    assert!(r.is_err(), "closed socket must surface an error");
}

#[tokio::test]
async fn unknown_event_lines_dont_resolve_the_pending_request() {
    let (_dir, socket, _tx) = spawn_misbehaving_mpv(Behavior::EmitOnlyEvents).await;
    let mut mpv = MpvController::with_socket_path(socket);
    mpv.connect_to_existing().await.unwrap();
    let r = tokio::time::timeout(Duration::from_millis(600), mpv.pause()).await;
    assert!(
        r.is_err(),
        "only-events stream must leave pause() pending until timeout"
    );
}
