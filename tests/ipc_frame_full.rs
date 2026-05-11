//! ipc/frame.rs: read_frame + lenient + size limits + closed.

use ferrosonic::ipc::frame::{
    read_frame_lenient, write_frame, Frame, FrameError, FrameRead, MAX_FRAME_BYTES,
};
use ferrosonic::ipc::protocol::{DaemonRequest, DaemonResponse};

#[tokio::test]
async fn read_frame_lenient_recognises_known_event() {
    let frame = Frame::Event(ferrosonic::ipc::protocol::DaemonEvent::Shutdown);
    let mut buf: Vec<u8> = Vec::new();
    write_frame(&mut buf, &frame).await.unwrap();
    let mut reader = buf.as_slice();
    let decoded = read_frame_lenient(&mut reader).await.unwrap();
    matches!(decoded, FrameRead::Ok(Frame::Event(_)));
}

#[tokio::test]
async fn read_frame_lenient_recognises_request() {
    let frame = Frame::Request {
        id: 99,
        req: DaemonRequest::Ping,
    };
    let mut buf: Vec<u8> = Vec::new();
    write_frame(&mut buf, &frame).await.unwrap();
    let mut reader = buf.as_slice();
    let decoded = read_frame_lenient(&mut reader).await.unwrap();
    matches!(decoded, FrameRead::Ok(Frame::Request { id: 99, .. }));
}

#[tokio::test]
async fn read_frame_lenient_unknown_request_variant() {
    let body = br#"{"Request":{"id":7,"req":{"FutureRequestType":{"x":1}}}}"#;
    let mut buf: Vec<u8> = Vec::new();
    buf.extend_from_slice(&(body.len() as u32).to_le_bytes());
    buf.extend_from_slice(body);
    let mut reader = buf.as_slice();
    let r = read_frame_lenient(&mut reader).await.unwrap();
    match r {
        FrameRead::UnknownRequest { id, .. } => assert_eq!(id, 7),
        other => panic!("expected UnknownRequest, got {:?}", other),
    }
}

#[tokio::test]
async fn read_frame_lenient_unknown_response_variant() {
    let body = br#"{"Response":{"id":3,"payload":{"Ok":{"FutureResponse":{}}}}}"#;
    let mut buf: Vec<u8> = Vec::new();
    buf.extend_from_slice(&(body.len() as u32).to_le_bytes());
    buf.extend_from_slice(body);
    let mut reader = buf.as_slice();
    let r = read_frame_lenient(&mut reader).await.unwrap();
    match r {
        FrameRead::UnknownResponse { id, .. } => assert_eq!(id, 3),
        other => panic!("expected UnknownResponse, got {:?}", other),
    }
}

#[tokio::test]
async fn read_frame_lenient_unknown_event_variant() {
    let body = br#"{"Event":{"FutureEvent":{"x":1}}}"#;
    let mut buf: Vec<u8> = Vec::new();
    buf.extend_from_slice(&(body.len() as u32).to_le_bytes());
    buf.extend_from_slice(body);
    let mut reader = buf.as_slice();
    let r = read_frame_lenient(&mut reader).await.unwrap();
    matches!(r, FrameRead::UnknownEvent { .. });
}

#[tokio::test]
async fn read_frame_lenient_returns_serialize_error_for_non_frame_json() {
    let body = br#"{"not_a_frame":true}"#;
    let mut buf: Vec<u8> = Vec::new();
    buf.extend_from_slice(&(body.len() as u32).to_le_bytes());
    buf.extend_from_slice(body);
    let mut reader = buf.as_slice();
    let r = read_frame_lenient(&mut reader).await;
    assert!(r.is_err());
}

#[tokio::test]
async fn read_frame_lenient_returns_closed_on_clean_eof() {
    let empty: Vec<u8> = Vec::new();
    let mut reader = empty.as_slice();
    let r = read_frame_lenient(&mut reader).await;
    matches!(r, Err(FrameError::Closed));
}

#[tokio::test]
async fn read_frame_lenient_returns_too_large_for_oversize_len() {
    let oversize = (MAX_FRAME_BYTES + 1) as u32;
    let mut buf: Vec<u8> = Vec::new();
    buf.extend_from_slice(&oversize.to_le_bytes());
    let mut reader = buf.as_slice();
    let r = read_frame_lenient(&mut reader).await;
    matches!(r, Err(FrameError::TooLarge(_)));
}

#[tokio::test]
async fn write_frame_rejects_oversize_when_body_exceeds_limit() {
    let huge = "x".repeat(MAX_FRAME_BYTES + 1000);
    let frame = Frame::Response {
        id: 1,
        payload: Err(huge),
    };
    let mut buf: Vec<u8> = Vec::new();
    let r = write_frame(&mut buf, &frame).await;
    matches!(r, Err(FrameError::TooLarge(_)));
}

#[tokio::test]
async fn round_trip_response_err_variant_preserves_message() {
    let frame = Frame::Response {
        id: 11,
        payload: Err("specific error text".into()),
    };
    let mut buf: Vec<u8> = Vec::new();
    write_frame(&mut buf, &frame).await.unwrap();
    let mut reader = buf.as_slice();
    let decoded = read_frame_lenient(&mut reader).await.unwrap();
    match decoded {
        FrameRead::Ok(Frame::Response {
            id: 11,
            payload: Err(msg),
        }) => assert_eq!(msg, "specific error text"),
        other => panic!("unexpected {:?}", other),
    }
}

#[tokio::test]
async fn round_trip_response_ok_pong() {
    let frame = Frame::Response {
        id: 1,
        payload: Ok(DaemonResponse::Pong),
    };
    let mut buf: Vec<u8> = Vec::new();
    write_frame(&mut buf, &frame).await.unwrap();
    let mut reader = buf.as_slice();
    let decoded = read_frame_lenient(&mut reader).await.unwrap();
    matches!(
        decoded,
        FrameRead::Ok(Frame::Response {
            id: 1,
            payload: Ok(_)
        })
    );
}

#[tokio::test]
async fn mid_frame_eof_returns_io_error() {
    let mut buf: Vec<u8> = Vec::new();
    buf.extend_from_slice(&100u32.to_le_bytes());
    buf.extend_from_slice(b"short");
    let mut reader = buf.as_slice();
    let r = read_frame_lenient(&mut reader).await;
    assert!(r.is_err());
}
