//! ipc/socket_client.rs: UnknownResponse / UnknownEvent / Request-from-daemon paths.

use ferrosonic::ipc::client::DaemonClient;
use ferrosonic::ipc::protocol::DaemonRequest;
use ferrosonic::ipc::SocketClient;
use serial_test::serial;
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::net::UnixListener;

async fn run_custom_server<F>(handler: F) -> std::path::PathBuf
where
    F: FnOnce(tokio::net::UnixStream) -> tokio::task::JoinHandle<()> + Send + 'static,
{
    let tmp = tempfile::tempdir().unwrap();
    let socket = tmp.path().join("custom.sock");
    let socket_clone = socket.clone();
    std::mem::forget(tmp);
    let listener = UnixListener::bind(&socket).unwrap();
    tokio::spawn(async move {
        if let Ok((stream, _)) = listener.accept().await {
            let h = handler(stream);
            let _ = h.await;
        }
    });
    socket_clone
}

fn write_raw_frame(buf: &mut Vec<u8>, body: &[u8]) {
    buf.extend_from_slice(&(body.len() as u32).to_le_bytes());
    buf.extend_from_slice(body);
}

#[tokio::test]
#[serial]
async fn unknown_response_variant_resolves_pending_with_err() {
    let socket = run_custom_server(|mut stream| {
        tokio::spawn(async move {
            let body = br#"{"Response":{"id":1,"payload":{"Ok":{"FutureRespVariant":{}}}}}"#;
            let mut buf = Vec::new();
            write_raw_frame(&mut buf, body);
            let _ = stream.write_all(&buf).await;
            tokio::time::sleep(Duration::from_millis(500)).await;
        })
    })
    .await;
    tokio::time::sleep(Duration::from_millis(50)).await;
    let client = SocketClient::connect(&socket).await.unwrap();
    let r = client.request(DaemonRequest::Ping).await;
    assert!(r.is_err());
}

#[tokio::test]
#[serial]
async fn unknown_event_variant_does_not_crash_subscriber() {
    let socket = run_custom_server(|mut stream| {
        tokio::spawn(async move {
            let body = br#"{"Event":{"FutureEvtVariant":{"x":1}}}"#;
            let mut buf = Vec::new();
            write_raw_frame(&mut buf, body);
            let _ = stream.write_all(&buf).await;
            tokio::time::sleep(Duration::from_millis(500)).await;
        })
    })
    .await;
    tokio::time::sleep(Duration::from_millis(50)).await;
    let client = SocketClient::connect(&socket).await.unwrap();
    let _rx = client.subscribe();
    tokio::time::sleep(Duration::from_millis(200)).await;
}

#[tokio::test]
#[serial]
async fn request_frame_from_daemon_is_ignored_by_client() {
    let socket = run_custom_server(|mut stream| {
        tokio::spawn(async move {
            let body = br#"{"Request":{"id":99,"req":"Ping"}}"#;
            let mut buf = Vec::new();
            write_raw_frame(&mut buf, body);
            let _ = stream.write_all(&buf).await;
            tokio::time::sleep(Duration::from_millis(500)).await;
        })
    })
    .await;
    tokio::time::sleep(Duration::from_millis(50)).await;
    let client = SocketClient::connect(&socket).await.unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;
    let _ = client.subscribe();
}

#[tokio::test]
#[serial]
async fn unknown_request_envelope_from_daemon_is_ignored_by_client() {
    let socket = run_custom_server(|mut stream| {
        tokio::spawn(async move {
            let body = br#"{"Request":{"id":42,"req":{"FutureReqType":{}}}}"#;
            let mut buf = Vec::new();
            write_raw_frame(&mut buf, body);
            let _ = stream.write_all(&buf).await;
            tokio::time::sleep(Duration::from_millis(500)).await;
        })
    })
    .await;
    tokio::time::sleep(Duration::from_millis(50)).await;
    let client = SocketClient::connect(&socket).await.unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;
    let _ = client.subscribe();
}

#[tokio::test]
#[serial]
async fn response_for_unknown_request_id_is_warned_and_dropped() {
    let socket = run_custom_server(|mut stream| {
        tokio::spawn(async move {
            let body = br#"{"Response":{"id":99999,"payload":{"Ok":"Pong"}}}"#;
            let mut buf = Vec::new();
            write_raw_frame(&mut buf, body);
            let _ = stream.write_all(&buf).await;
            tokio::time::sleep(Duration::from_millis(500)).await;
        })
    })
    .await;
    tokio::time::sleep(Duration::from_millis(50)).await;
    let _client = SocketClient::connect(&socket).await.unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;
}

#[tokio::test]
#[serial]
async fn server_closes_socket_resolves_pending_with_disconnected() {
    let socket = run_custom_server(|stream| {
        tokio::spawn(async move {
            drop(stream);
        })
    })
    .await;
    tokio::time::sleep(Duration::from_millis(50)).await;
    let client = SocketClient::connect(&socket).await.unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    let r = tokio::time::timeout(
        Duration::from_millis(500),
        client.request(DaemonRequest::Ping),
    )
    .await;
    if let Ok(inner) = r {
        assert!(inner.is_err());
    }
}
