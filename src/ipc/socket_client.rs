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
/// Upper bound on waiting for one reply. A daemon handler that wedges must not
/// freeze the TUI forever; callers surface [`IpcError::Timeout`].
const REQUEST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);
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
    ///
    /// # Errors
    /// Returns an `IpcError` if the connection cannot be established.
    pub async fn connect(path: &Path) -> Result<Arc<Self>, IpcError> {
        let stream = UnixStream::connect(path).await?;
        let (read_half, mut write_half) = stream.into_split();

        let (writer_tx, mut writer_rx) = mpsc::channel::<Frame>(WRITER_QUEUE_DEPTH);
        let (event_tx, _) = broadcast::channel(EVENT_CHANNEL_CAPACITY);
        let pending: Arc<PendingMap> = Arc::new(Mutex::new(HashMap::new()));

        let client = Arc::new(Self {
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

        let reader_pending = pending;
        let reader_events = event_tx;
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
                                "unknown response variant: {body}"
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
                        // Notify subscribers so the TUI quits with an explicit
                        // error instead of rendering a stale state mirror.
                        let _ = reader_events.send(DaemonEvent::Shutdown);
                        break;
                    }
                    Err(e) => {
                        error!("Frame read error, terminating reader: {}", e);
                        let _ = reader_events.send(DaemonEvent::Shutdown);
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

    async fn request_with_timeout(
        &self,
        req: DaemonRequest,
        timeout: std::time::Duration,
    ) -> Result<DaemonResponse, IpcError> {
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
            self.pending.lock().await.remove(&id);

            return Err(IpcError::Disconnected);
        }
        match tokio::time::timeout(timeout, rx).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => Err(IpcError::Disconnected),
            Err(_) => {
                // Drop the pending entry so a late reply is ignored rather than
                // warning about a removed id.
                self.pending.lock().await.remove(&id);
                Err(IpcError::Timeout)
            }
        }
    }
}

#[async_trait]
impl DaemonClient for SocketClient {
    #[allow(clippy::option_if_let_else)] // recv result -> Disconnected; explicit arms clearer.
    async fn request(&self, req: DaemonRequest) -> Result<DaemonResponse, IpcError> {
        self.request_with_timeout(req, REQUEST_TIMEOUT).await
    }

    fn subscribe(&self) -> broadcast::Receiver<DaemonEvent> {
        self.event_tx.subscribe()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::UnixListener;

    #[tokio::test(start_paused = true)]
    async fn late_response_is_discarded_and_pending_returns_to_baseline() {
        let dir = tempfile::tempdir().expect("temporary directory");
        let path = dir.path().join("late-response.sock");
        let listener = UnixListener::bind(&path).expect("bind test listener");

        tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.expect("accept client");
            let first_id = match read_frame_lenient(&mut stream)
                .await
                .expect("read first request")
            {
                FrameRead::Ok(Frame::Request { id, .. }) => id,
                other => panic!("expected first request, got {other:?}"),
            };

            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            write_frame(
                &mut stream,
                &Frame::Response {
                    id: first_id,
                    payload: Ok(DaemonResponse::Pong),
                },
            )
            .await
            .expect("write deliberately late response");

            let second_id = match read_frame_lenient(&mut stream)
                .await
                .expect("read second request")
            {
                FrameRead::Ok(Frame::Request { id, .. }) => id,
                other => panic!("expected second request, got {other:?}"),
            };
            write_frame(
                &mut stream,
                &Frame::Response {
                    id: second_id,
                    payload: Ok(DaemonResponse::Pong),
                },
            )
            .await
            .expect("write current response");
        });

        let client = SocketClient::connect(&path).await.expect("connect client");
        let first = client
            .request_with_timeout(DaemonRequest::Ping, std::time::Duration::from_secs(1))
            .await;
        assert!(matches!(first, Err(IpcError::Timeout)));
        assert_eq!(client.pending.lock().await.len(), 0);

        let second = client.request(DaemonRequest::Ping).await;
        assert!(matches!(second, Ok(DaemonResponse::Pong)));
        assert_eq!(client.pending.lock().await.len(), 0);
    }
}
