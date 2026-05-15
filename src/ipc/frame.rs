//! Wire format: `u32 LE length || JSON body`. JSON over bincode for
//! debuggability with `socat`; throughput is plenty at this scale.

// Variants span single bool to full snapshot; boxing helps nothing.

use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::ipc::protocol::{DaemonEvent, DaemonRequest, DaemonResponse};

pub const MAX_FRAME_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_REQUEST_FRAME_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Frame {
    Request {
        id: u64,
        req: DaemonRequest,
    },
    /// `payload` is `Result<DaemonResponse, String>` so daemon-side
    /// errors round-trip without per-variant wire encoding.
    Response {
        id: u64,
        payload: Result<DaemonResponse, String>,
    },
    Event(DaemonEvent),
}

/// Result of a tolerant frame parse. Splits "couldn't parse the
/// envelope" (fatal) from "envelope OK, payload type unknown" so the
/// reader can recover from forward/back protocol mismatches without
/// severing the connection.
#[derive(Debug)]
pub enum FrameRead {
    /// Fully parsed; payload variant was known.
    Ok(Frame),
    /// Envelope parsed but the request body was an unknown variant.
    /// Daemon should reply with `Response { id, payload: Err(...) }`.
    UnknownRequest { id: u64, body: String },
    /// Envelope parsed but the response body was an unknown variant.
    /// Client should resolve pending request `id` with an error.
    UnknownResponse { id: u64, body: String },
    /// Envelope parsed but the event was an unknown variant. Receiver
    /// should log and continue.
    UnknownEvent { body: String },
}

#[derive(Debug, thiserror::Error)]
pub enum FrameError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("frame too large: {0} bytes")]
    TooLarge(usize),
    #[error("serialize: {0}")]
    Serialize(#[from] serde_json::Error),
    #[error("peer closed before frame complete")]
    Closed,
}

/// `Closed` is clean EOF between frames; distinct from mid-frame disconnects which surface as `Io(UnexpectedEof)`.
///
/// ```
/// use ferrosonic::ipc::frame::{read_frame, write_frame, Frame};
/// use ferrosonic::ipc::protocol::DaemonRequest;
/// tokio_test::block_on(async {
///     let original = Frame::Request { id: 42, req: DaemonRequest::Ping };
///     let mut buf: Vec<u8> = Vec::new();
///     write_frame(&mut buf, &original).await.unwrap();
///     let mut reader = buf.as_slice();
///     let decoded = read_frame(&mut reader).await.unwrap();
///     match decoded {
///         Frame::Request { id, .. } => assert_eq!(id, 42),
///         other => panic!("expected Request, got {:?}", other),
///     }
/// });
/// ```
pub async fn read_frame<R>(reader: &mut R) -> Result<Frame, FrameError>
where
    R: AsyncReadExt + Unpin,
{
    let body = read_frame_body(reader).await?;
    let frame: Frame = serde_json::from_slice(&body)?;
    Ok(frame)
}

/// Reads the next frame and attempts a typed parse; on unknown variants returns the envelope metadata so the caller can keep the connection alive.
///
/// ```
/// use ferrosonic::ipc::frame::{read_frame_lenient, FrameRead};
/// tokio_test::block_on(async {
///     let body = serde_json::json!({
///         "Request": { "id": 99, "req": { "TotallyNewCommand": "hello" } }
///     }).to_string();
///     let mut buf = Vec::new();
///     buf.extend_from_slice(&(body.len() as u32).to_le_bytes());
///     buf.extend_from_slice(body.as_bytes());
///     let mut reader = buf.as_slice();
///     match read_frame_lenient(&mut reader).await.unwrap() {
///         FrameRead::UnknownRequest { id, .. } => assert_eq!(id, 99),
///         other => panic!("expected UnknownRequest, got {:?}", other),
///     }
/// });
/// ```
pub async fn read_frame_lenient<R>(reader: &mut R) -> Result<FrameRead, FrameError>
where
    R: AsyncReadExt + Unpin,
{
    read_frame_lenient_with_cap(reader, MAX_FRAME_BYTES).await
}

/// Like [`read_frame_lenient`] but enforces a caller-supplied length cap. Used by the daemon to apply a tighter [`MAX_REQUEST_FRAME_BYTES`] than the global [`MAX_FRAME_BYTES`] on inbound requests.
///
/// ```
/// use ferrosonic::ipc::frame::{read_frame_lenient_with_cap, FrameError};
/// tokio_test::block_on(async {
///     let oversized_len: u32 = 65;
///     let cap: usize = 32;
///     let mut buf = Vec::new();
///     buf.extend_from_slice(&oversized_len.to_le_bytes());
///     let mut reader = buf.as_slice();
///     let err = read_frame_lenient_with_cap(&mut reader, cap).await.unwrap_err();
///     assert!(matches!(err, FrameError::TooLarge(n) if n == oversized_len as usize));
/// });
/// ```
pub async fn read_frame_lenient_with_cap<R>(
    reader: &mut R,
    cap: usize,
) -> Result<FrameRead, FrameError>
where
    R: AsyncReadExt + Unpin,
{
    let body = read_frame_body_with_cap(reader, cap).await?;

    let typed_err = match serde_json::from_slice::<Frame>(&body) {
        Ok(frame) => return Ok(FrameRead::Ok(frame)),
        Err(e) => e,
    };

    let raw: serde_json::Value = serde_json::from_slice(&body)?;

    if let Some(req) = raw.get("Request") {
        if let Some(id) = req.get("id").and_then(|v| v.as_u64()) {
            let inner = req
                .get("req")
                .map(|v| v.to_string())
                .unwrap_or_else(|| "<missing>".into());
            return Ok(FrameRead::UnknownRequest { id, body: inner });
        }
    }
    if let Some(resp) = raw.get("Response") {
        if let Some(id) = resp.get("id").and_then(|v| v.as_u64()) {
            let inner = resp
                .get("payload")
                .map(|v| v.to_string())
                .unwrap_or_else(|| "<missing>".into());
            return Ok(FrameRead::UnknownResponse { id, body: inner });
        }
    }
    if let Some(ev) = raw.get("Event") {
        return Ok(FrameRead::UnknownEvent {
            body: ev.to_string(),
        });
    }

    Err(FrameError::Serialize(typed_err))
}

async fn read_frame_body<R>(reader: &mut R) -> Result<Vec<u8>, FrameError>
where
    R: AsyncReadExt + Unpin,
{
    read_frame_body_with_cap(reader, MAX_FRAME_BYTES).await
}

async fn read_frame_body_with_cap<R>(reader: &mut R, cap: usize) -> Result<Vec<u8>, FrameError>
where
    R: AsyncReadExt + Unpin,
{
    let mut len_buf = [0u8; 4];
    match reader.read_exact(&mut len_buf).await {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
            return Err(FrameError::Closed);
        }
        Err(e) => return Err(FrameError::Io(e)),
    }
    let len = u32::from_le_bytes(len_buf) as usize;
    if len > cap {
        return Err(FrameError::TooLarge(len));
    }
    let mut body = vec![0u8; len];
    reader.read_exact(&mut body).await?;
    Ok(body)
}

/// Single combined write so partial frames never reach the wire.
pub async fn write_frame<W>(writer: &mut W, frame: &Frame) -> Result<(), FrameError>
where
    W: AsyncWriteExt + Unpin,
{
    let body = serde_json::to_vec(frame)?;
    if body.len() > MAX_FRAME_BYTES {
        return Err(FrameError::TooLarge(body.len()));
    }
    let mut buf = Vec::with_capacity(4 + body.len());
    buf.extend_from_slice(&(body.len() as u32).to_le_bytes());
    buf.extend_from_slice(&body);
    writer.write_all(&buf).await?;
    writer.flush().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ipc::protocol::DaemonRequest;

    #[tokio::test]
    async fn frame_roundtrip_request() {
        let original = Frame::Request {
            id: 42,
            req: DaemonRequest::TogglePause,
        };
        let mut buf: Vec<u8> = Vec::new();
        write_frame(&mut buf, &original).await.unwrap();
        let mut reader = buf.as_slice();
        let decoded = read_frame(&mut reader).await.unwrap();
        match decoded {
            Frame::Request { id, req: _ } => assert_eq!(id, 42),
            _ => panic!("expected Request, got {:?}", decoded),
        }
    }

    #[tokio::test]
    async fn frame_roundtrip_response_ok() {
        let original = Frame::Response {
            id: 7,
            payload: Ok(DaemonResponse::Pong),
        };
        let mut buf: Vec<u8> = Vec::new();
        write_frame(&mut buf, &original).await.unwrap();
        let mut reader = buf.as_slice();
        let decoded = read_frame(&mut reader).await.unwrap();
        match decoded {
            Frame::Response { id, payload: Ok(_) } => assert_eq!(id, 7),
            _ => panic!("expected Response Ok, got {:?}", decoded),
        }
    }

    #[tokio::test]
    async fn frame_roundtrip_response_err() {
        let original = Frame::Response {
            id: 9,
            payload: Err("boom".to_string()),
        };
        let mut buf: Vec<u8> = Vec::new();
        write_frame(&mut buf, &original).await.unwrap();
        let mut reader = buf.as_slice();
        let decoded = read_frame(&mut reader).await.unwrap();
        match decoded {
            Frame::Response {
                id,
                payload: Err(msg),
            } => {
                assert_eq!(id, 9);
                assert_eq!(msg, "boom");
            }
            _ => panic!("expected Response Err, got {:?}", decoded),
        }
    }

    #[tokio::test]
    async fn frame_clean_eof_is_closed() {
        let mut empty: &[u8] = &[];
        let err = read_frame(&mut empty).await.unwrap_err();
        assert!(matches!(err, FrameError::Closed));
    }

    #[tokio::test]
    async fn frame_too_large_rejected() {
        let mut buf: Vec<u8> = Vec::new();
        buf.extend_from_slice(&((MAX_FRAME_BYTES as u32 + 1).to_le_bytes()));
        let mut reader = buf.as_slice();
        let err = read_frame(&mut reader).await.unwrap_err();
        assert!(matches!(err, FrameError::TooLarge(_)));
    }

    #[tokio::test]
    async fn lenient_unknown_request_returns_id() {
        let body = serde_json::json!({
            "Request": {
                "id": 99,
                "req": { "TotallyNewCommand": "hello" }
            }
        })
        .to_string();
        let len = (body.len() as u32).to_le_bytes();
        let mut buf = Vec::new();
        buf.extend_from_slice(&len);
        buf.extend_from_slice(body.as_bytes());
        let mut reader = buf.as_slice();
        match read_frame_lenient(&mut reader).await.unwrap() {
            FrameRead::UnknownRequest { id, .. } => assert_eq!(id, 99),
            other => panic!("expected UnknownRequest, got {:?}", other),
        }
    }
}
