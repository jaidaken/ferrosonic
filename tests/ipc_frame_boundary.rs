//! Boundary + arithmetic mutation kills for src/ipc/frame.rs (F3 survivors).

use ferrosonic::ipc::frame::{
    read_frame_lenient, read_frame_lenient_with_cap, write_frame, Frame, FrameError,
    MAX_FRAME_BYTES, MAX_REQUEST_FRAME_BYTES,
};
use ferrosonic::ipc::protocol::DaemonResponse;
use std::io;

#[test]
fn max_frame_bytes_pins_to_16_mib() {
    assert_eq!(MAX_FRAME_BYTES, 16 * 1024 * 1024);
    assert_eq!(MAX_FRAME_BYTES, 16_777_216);
}

#[test]
fn max_request_frame_bytes_pins_to_1_mib() {
    assert_eq!(MAX_REQUEST_FRAME_BYTES, 1024 * 1024);
    assert_eq!(MAX_REQUEST_FRAME_BYTES, 1_048_576);
}

#[tokio::test]
async fn read_frame_propagates_non_eof_io_error() {
    let mock = tokio_test::io::Builder::new()
        .read_error(io::Error::new(io::ErrorKind::PermissionDenied, "denied"))
        .build();
    let mut reader = mock;
    let r = read_frame_lenient(&mut reader).await;
    match r {
        Err(FrameError::Io(e)) => {
            assert_eq!(e.kind(), io::ErrorKind::PermissionDenied);
        }
        other => panic!("expected FrameError::Io(PermissionDenied), got {:?}", other),
    }
}

#[tokio::test]
async fn read_frame_eof_remains_closed_not_io() {
    let mock = tokio_test::io::Builder::new()
        .read_error(io::Error::new(io::ErrorKind::UnexpectedEof, "eof"))
        .build();
    let mut reader = mock;
    let r = read_frame_lenient(&mut reader).await;
    assert!(
        matches!(r, Err(FrameError::Closed)),
        "expected FrameError::Closed, got {:?}",
        r
    );
}

#[tokio::test]
async fn read_frame_body_accepts_exactly_cap_bytes() {
    let cap: usize = 128;
    let body = vec![b'x'; cap];
    let mut buf: Vec<u8> = Vec::new();
    buf.extend_from_slice(&(cap as u32).to_le_bytes());
    buf.extend_from_slice(&body);
    let mut reader = buf.as_slice();
    let r = read_frame_lenient_with_cap(&mut reader, cap).await;
    match r {
        Err(FrameError::TooLarge(n)) => {
            panic!("cap == len must NOT trigger TooLarge; got TooLarge({})", n)
        }
        Err(FrameError::Serialize(_)) => {}
        Err(other) => panic!("expected Serialize error (body not JSON), got {:?}", other),
        Ok(_) => panic!("body is not valid JSON; expected Serialize error"),
    }
}

#[tokio::test]
async fn read_frame_body_rejects_one_over_cap() {
    let cap: usize = 128;
    let len: u32 = cap as u32 + 1;
    let mut buf: Vec<u8> = Vec::new();
    buf.extend_from_slice(&len.to_le_bytes());
    let mut reader = buf.as_slice();
    let r = read_frame_lenient_with_cap(&mut reader, cap).await;
    match r {
        Err(FrameError::TooLarge(n)) => assert_eq!(n, cap + 1),
        other => panic!("expected TooLarge({}), got {:?}", cap + 1, other),
    }
}

#[tokio::test]
async fn write_frame_accepts_body_exactly_max_frame_bytes() {
    let frame_at_max = build_frame_with_body_len(MAX_FRAME_BYTES);
    let serialized_len = serde_json::to_vec(&frame_at_max).unwrap().len();
    assert_eq!(
        serialized_len, MAX_FRAME_BYTES,
        "test setup: serialized body must equal MAX_FRAME_BYTES exactly"
    );
    let mut buf: Vec<u8> = Vec::new();
    let r = write_frame(&mut buf, &frame_at_max).await;
    assert!(
        r.is_ok(),
        "write_frame must accept body == MAX_FRAME_BYTES; got {:?}",
        r
    );
    assert_eq!(buf.len(), 4 + MAX_FRAME_BYTES);
}

#[tokio::test]
async fn write_frame_rejects_body_one_over_max_frame_bytes() {
    let frame_over = build_frame_with_body_len(MAX_FRAME_BYTES + 1);
    let serialized_len = serde_json::to_vec(&frame_over).unwrap().len();
    assert_eq!(
        serialized_len,
        MAX_FRAME_BYTES + 1,
        "test setup: serialized body must equal MAX_FRAME_BYTES+1 exactly"
    );
    let mut buf: Vec<u8> = Vec::new();
    let r = write_frame(&mut buf, &frame_over).await;
    match r {
        Err(FrameError::TooLarge(n)) => assert_eq!(n, MAX_FRAME_BYTES + 1),
        other => panic!(
            "expected TooLarge({}), got {:?}",
            MAX_FRAME_BYTES + 1,
            other
        ),
    }
}

#[tokio::test]
async fn read_frame_accepts_body_exactly_max_frame_bytes_via_default_cap() {
    let payload_len = MAX_FRAME_BYTES;
    let body = vec![b'x'; payload_len];
    let mut buf: Vec<u8> = Vec::new();
    buf.extend_from_slice(&(payload_len as u32).to_le_bytes());
    buf.extend_from_slice(&body);
    let mut reader = buf.as_slice();
    let r = read_frame_lenient(&mut reader).await;
    match r {
        Err(FrameError::TooLarge(n)) => panic!(
            "len == MAX_FRAME_BYTES must NOT trigger TooLarge; got TooLarge({})",
            n
        ),
        Err(FrameError::Serialize(_)) => {}
        Err(other) => panic!("expected Serialize (body not JSON), got {:?}", other),
        Ok(_) => panic!("body is not valid JSON; expected Serialize"),
    }
}

fn build_frame_with_body_len(target: usize) -> Frame {
    let probe = Frame::Response {
        id: 0u64,
        payload: Err(String::new()),
    };
    let probe_len = serde_json::to_vec(&probe).unwrap().len();
    assert!(
        target >= probe_len,
        "target {} smaller than envelope {}",
        target,
        probe_len
    );
    let pad = target - probe_len;
    let frame = Frame::Response {
        id: 0u64,
        payload: Err("x".repeat(pad)),
    };
    let actual = serde_json::to_vec(&frame).unwrap().len();
    assert_eq!(
        actual, target,
        "padding math wrong: produced {} not {}",
        actual, target
    );
    frame
}

#[test]
fn build_frame_helper_is_exact() {
    let f = build_frame_with_body_len(1024);
    assert_eq!(serde_json::to_vec(&f).unwrap().len(), 1024);
    let f2 = build_frame_with_body_len(65536);
    assert_eq!(serde_json::to_vec(&f2).unwrap().len(), 65536);
}

#[tokio::test]
async fn write_frame_pong_response_succeeds_well_below_cap() {
    let frame = Frame::Response {
        id: 1,
        payload: Ok(DaemonResponse::Pong),
    };
    let mut buf: Vec<u8> = Vec::new();
    write_frame(&mut buf, &frame).await.unwrap();
    assert!(buf.len() > 4);
    assert!(buf.len() < 1024);
}
