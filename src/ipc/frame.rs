//! Length-prefixed JSON frame I/O over a Unix domain socket.
//!
//! Wire format: `u32 LE length || JSON body`. The body is a `Frame`
//! tagged enum that multiplexes the three logical streams on one
//! socket:
//!
//! - `Request { id, req }`: client → daemon. The `id` is a monotonic
//!   per-connection counter the client allocates; the daemon echoes
//!   it back on the matching response.
//! - `Response { id, payload }`: daemon → client, paired with a
//!   request `id`. `payload` is `Result<DaemonResponse, String>` so
//!   daemon-side errors round-trip without the wire schema needing a
//!   separate variant for every error type.
//! - `Event(DaemonEvent)`: daemon → client, unsolicited. The client
//!   side routes these into its local mirror update path.
//!
//! Why JSON over `bincode`? Debuggability with `socat -d -d
//! UNIX-CONNECT:$socket -` matters more than throughput at this
//! scale (tens of frames per second peak). JSON also tolerates
//! protocol additions (new optional fields) without schema bumps.
//!
//! Why u32 length? More than 4GB of one frame is never sensible; u32
//! gives a sharp cap that catches pathological deserialisation early.
//! `MAX_FRAME_BYTES` enforces a much lower hard limit.

#![allow(dead_code)] // wired into SocketClient/SocketServer in subsequent commits

use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::ipc::protocol::{DaemonEvent, DaemonRequest, DaemonResponse};

/// Hard cap on per-frame payload size. 16 MiB is generous for a
/// `LoadAlbum` response carrying a full album's metadata; anything
/// larger is almost certainly a bug or attack.
pub const MAX_FRAME_BYTES: usize = 16 * 1024 * 1024;

/// One wire message.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum Frame {
    /// Client-to-daemon command. `id` correlates with the matching
    /// `Response`.
    Request {
        id: u64,
        req: DaemonRequest,
    },
    /// Daemon-to-client reply. `payload` carries the typed response or
    /// a string-rendered error.
    Response {
        id: u64,
        payload: Result<DaemonResponse, String>,
    },
    /// Daemon-to-client server-pushed event.
    Event(DaemonEvent),
}

/// Errors from frame I/O. Distinct from `IpcError` because frame
/// errors are lower-level (transport / encoding).
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

/// Read one frame from `reader`. Returns `Closed` if the peer hung
/// up cleanly before any bytes of a new frame arrived (clean EOF
/// between frames is normal — this lets the caller distinguish
/// "connection ended" from "connection broken mid-frame").
pub async fn read_frame<R>(reader: &mut R) -> Result<Frame, FrameError>
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
    if len > MAX_FRAME_BYTES {
        return Err(FrameError::TooLarge(len));
    }
    let mut body = vec![0u8; len];
    reader.read_exact(&mut body).await?;
    let frame: Frame = serde_json::from_slice(&body)?;
    Ok(frame)
}

/// Write one frame to `writer`. Flushes the length and payload as
/// a single buffer write so partial frames never appear on the wire.
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
}
