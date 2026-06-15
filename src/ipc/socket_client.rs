//! `DaemonClient` over a Unix socket. Separate reader + writer tasks;
//! the reader demuxes Response (resolves pending) vs Event (broadcasts).


use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use tokio::io::AsyncWriteExt;
use tokio::net::UnixStream;
use tokio::sync::{broadcast, mpsc, oneshot, Mutex};
use tracing::{debug, error, warn};

use crate::ipc::frame::{read_frame_lenient, write_frame, Frame, FrameError, FrameRead};
use crate::ipc::protocol::{DaemonEvent, DaemonRequest, DaemonResponse, IpcError};
use crate::ipc::DaemonClient;

const EVENT_CHANNEL_CAPACITY: usize = 256;
const WRITER_QUEUE_DEPTH: usize = 256;
/// Keepalive cadence. The daemon closes a connection idle for
/// `IDLE_TIMEOUT` (`server.rs`); this stays well under a third of it so a
/// live-but-quiet TUI never trips that timeout.
const PING_INTERVAL: std::time::Duration = std::time::Duration::from_secs(15);

type PendingMap = Mutex<HashMap<u64, oneshot::Sender<Result<DaemonResponse, IpcError>>>>;

/// `DaemonClient` over the Unix socket to a separate `ferrosonicd` process.
pub struct SocketClient {
    next_id: AtomicU64,
    writer_tx: mpsc::Sender<Frame>,
    pending: Arc<PendingMap>,
    event_tx: broadcast::Sender<DaemonEvent>,
}

impl SocketClient {
    /// Connect to the daemon socket and spawn the reader/writer tasks.
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

        tokio::spawn(async move {
            while let Some(frame) = writer_rx.recv().await {
                if let Err(e) = write_frame(&mut write_half, &frame).await {
                    error!("Socket write failed, terminating writer: {}", e);
                    break;
                }
            }
            let _ = write_half.shutdown().await;
        });

        let reader_pending = pending.clone();
        let reader_events = event_tx.clone();
        tokio::spawn(async move {
            let mut reader = read_half;
            loop {
                match read_frame_lenient(&mut reader).await {
                    Ok(FrameRead::Ok(Frame::Response { id, payload })) => {
                        let mut map = reader_pending.lock().await;
                        if let Some(tx) = map.remove(&id) {
                            let result = payload.map_err(IpcError::Daemon);
                            let _ = tx.send(result);
                        } else {
                            warn!("Got response for unknown request id {}", id);
                        }
                    }
                    Ok(FrameRead::Ok(Frame::Event(ev))) => {
                        let _ = reader_events.send(ev);
                    }
                    Ok(FrameRead::Ok(Frame::Request { .. })) => {
                        warn!("Daemon sent a Request frame, ignoring");
                    }
                    Ok(FrameRead::UnknownResponse { id, body }) => {
                        warn!("Unknown response variant from daemon (id={}); resolving pending with Err: {}", id, body);
                        let mut map = reader_pending.lock().await;
                        if let Some(tx) = map.remove(&id) {
                            let _ = tx.send(Err(IpcError::Daemon(format!(
                                "unknown response variant: {}",
                                body
                            ))));
                        }
                    }
                    Ok(FrameRead::UnknownEvent { body }) => {
                        warn!("Unknown event variant from daemon, ignoring: {}", body);
                    }
                    Ok(FrameRead::UnknownRequest { id, .. }) => {
                        warn!("Daemon sent a Request frame (id={}), ignoring", id);
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
            let mut map = reader_pending.lock().await;
            for (_, tx) in map.drain() {
                let _ = tx.send(Err(IpcError::Disconnected));
            }
        });

        // Keepalive: a Weak handle so this task never keeps the client alive.
        // Exits when the client is dropped or a ping fails (daemon gone).
        let ping_client = Arc::downgrade(&client);
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(PING_INTERVAL);
            tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            tick.tick().await;
            loop {
                tick.tick().await;
                let Some(client) = ping_client.upgrade() else {
                    break;
                };
                if client.request(DaemonRequest::Ping).await.is_err() {
                    break;
                }
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
        match rx.await {
            Ok(result) => result,
            Err(_) => Err(IpcError::Disconnected),
        }
    }

    fn subscribe(&self) -> broadcast::Receiver<DaemonEvent> {
        self.event_tx.subscribe()
    }
}
