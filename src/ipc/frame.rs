//! Wire format: `u32 LE length || JSON body`. JSON over bincode for
//! debuggability with `socat`; throughput is plenty at this scale.

#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::ipc::protocol::{DaemonEvent, DaemonRequest, DaemonResponse};

pub const MAX_FRAME_BYTES: usize = 16 * 1024 * 1024;

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

/// `Closed` is clean EOF between frames; distinct from mid-frame
/// disconnects which surface as `Io(UnexpectedEof)`.
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
}
