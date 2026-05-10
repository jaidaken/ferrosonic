//! `DaemonClient` implementation that talks to `ferrosonicd` over a
//! Unix domain socket. The shape mirrors `InProcessClient` exactly —
//! call sites in the TUI don't change between the two.
//!
//! Concurrency model: one writer task owns the write half of the
//! socket; one reader task owns the read half. The reader demuxes
//! incoming frames:
//! - `Frame::Response { id, payload }` → look up the matching
//!   one-shot in `pending` and resolve it.
//! - `Frame::Event(_)` → push to the broadcast sender; subscribers
//!   in the TUI consume from `subscribe()`.
//! - `Frame::Request { .. }` → unexpected; logged + ignored.
//!
//! Cleanup: dropping `SocketClient` drops the writer mpsc, which
//! ends the writer task; the writer dropping the write half closes
//! the socket; the reader sees EOF and exits. All pending requests
//! resolve to `IpcError::Disconnected`.

#![allow(dead_code)] // wired up in phase 5 binary split

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use tokio::io::AsyncWriteExt;
use tokio::net::UnixStream;
use tokio::sync::{broadcast, mpsc, oneshot, Mutex};
use tracing::{debug, error, warn};

use crate::ipc::frame::{read_frame, write_frame, Frame, FrameError};
use crate::ipc::protocol::{DaemonEvent, DaemonRequest, DaemonResponse, IpcError};
use crate::ipc::DaemonClient;

/// Capacity of the broadcast channel for events forwarded to TUI
/// subscribers. Matches `DaemonCore::EVENT_CHANNEL_CAPACITY`.
const EVENT_CHANNEL_CAPACITY: usize = 256;

/// Capacity of the writer mpsc. Generous because requests are tiny
/// and the writer task drains promptly.
const WRITER_QUEUE_DEPTH: usize = 256;

type PendingMap = Mutex<HashMap<u64, oneshot::Sender<Result<DaemonResponse, IpcError>>>>;

/// Socket-backed DaemonClient. Construct with `connect()`.
pub struct SocketClient {
    next_id: AtomicU64,
    writer_tx: mpsc::Sender<Frame>,
    pending: Arc<PendingMap>,
    event_tx: broadcast::Sender<DaemonEvent>,
}

impl SocketClient {
    /// Connect to `ferrosonicd` at `path`. Spawns the reader and writer
    /// tasks; they run until the socket closes or the client is dropped.
    pub async fn connect(path: &Path) -> Result<Arc<Self>, IpcError> {
        let stream = UnixStream::connect(path).await?;
        let (read_half, mut write_half) = stream.into_split();

        let (writer_tx, mut writer_rx) = mpsc::channel::<Frame>(WRITER_QUEUE_DEPTH);
        let (event_tx, _) = broadcast::channel(EVENT_CHANNEL_CAPACITY);
        let pending: Arc<PendingMap> = Arc::new(Mutex::new(HashMap::new()));

        let client = Arc::new(SocketClient {
            next_id: AtomicU64::new(1),
            writer_tx,
            pending: pending.clone(),
            event_tx: event_tx.clone(),
        });

        // Writer task: drain mpsc, write to socket. Exits cleanly when
        // the mpsc is dropped (i.e., when the SocketClient is dropped).
        tokio::spawn(async move {
            while let Some(frame) = writer_rx.recv().await {
                if let Err(e) = write_frame(&mut write_half, &frame).await {
                    error!("Socket write failed, terminating writer: {}", e);
                    break;
                }
            }
            // Best-effort half-close so the daemon sees EOF on its read side.
            let _ = write_half.shutdown().await;
        });

        // Reader task: demux incoming frames into pending responses
        // and the event broadcast.
        let reader_pending = pending.clone();
        let reader_events = event_tx.clone();
        tokio::spawn(async move {
            let mut reader = read_half;
            loop {
                match read_frame(&mut reader).await {
                    Ok(Frame::Response { id, payload }) => {
                        let mut map = reader_pending.lock().await;
                        if let Some(tx) = map.remove(&id) {
                            let result = payload.map_err(IpcError::Daemon);
                            let _ = tx.send(result);
                        } else {
                            warn!("Got response for unknown request id {}", id);
                        }
                    }
                    Ok(Frame::Event(ev)) => {
                        let _ = reader_events.send(ev);
                    }
                    Ok(Frame::Request { .. }) => {
                        warn!("Daemon sent a Request frame, ignoring");
                    }
                    Err(FrameError::Closed) => {
                        debug!("Daemon socket closed cleanly");
                        break;
                    }
                    Err(e) => {
                        error!("Frame read error, terminating reader: {}", e);
                        break;
                    }
                }
            }
            // Resolve everything pending so callers don't hang forever.
            let mut map = reader_pending.lock().await;
            for (_, tx) in map.drain() {
                let _ = tx.send(Err(IpcError::Disconnected));
            }
        });

        Ok(client)
    }
}

#[async_trait]
impl DaemonClient for SocketClient {
    async fn request(&self, req: DaemonRequest) -> Result<DaemonResponse, IpcError> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = oneshot::channel();
        {
            let mut map = self.pending.lock().await;
            map.insert(id, tx);
        }
        // Send the frame; on failure clean up the pending slot.
        if self
            .writer_tx
            .send(Frame::Request { id, req })
            .await
            .is_err()
        {
            let mut map = self.pending.lock().await;
            map.remove(&id);
            return Err(IpcError::Disconnected);
        }
        // Wait for the reader to resolve this request.
        match rx.await {
            Ok(result) => result,
            Err(_) => Err(IpcError::Disconnected),
        }
    }

    fn subscribe(&self) -> broadcast::Receiver<DaemonEvent> {
        self.event_tx.subscribe()
    }
}
