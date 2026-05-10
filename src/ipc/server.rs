//! Daemon-side IPC server. Listens on the Unix socket, spawns a
//! per-connection task that:
//! - reads `Frame::Request` frames from the client,
//! - dispatches each through an `InProcessClient` (the same type the
//!   single-process build uses) to `DaemonCore`,
//! - writes the matching `Frame::Response` back, with the original
//!   request `id` echoed,
//! - subscribes to `DaemonCore::event_tx` and writes every event as
//!   `Frame::Event` until the connection closes.
//!
//! Concurrency: one task per connected client, isolated from the
//! others. A misbehaving client (slow read, disconnect mid-frame)
//! cannot block any other client or the daemon's own tasks.
//!
//! Stale-socket handling: if the socket file already exists at
//! bind time, we attempt to connect to it; if the connection
//! succeeds, another daemon is running and we return `AddrInUse`.
//! If it fails, we unlink and retry. This avoids zombies left by
//! `kill -9` of a previous daemon.

#![allow(dead_code)] // wired into ferrosonicd binary in phase 5

use std::path::Path;
use std::sync::Arc;

use tokio::io::AsyncWriteExt;
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::broadcast;
use tracing::{debug, error, info, warn};

use crate::daemon::DaemonCore;
use crate::ipc::client::InProcessClient;
use crate::ipc::frame::{read_frame, write_frame, Frame, FrameError};
use crate::ipc::path::ensure_parent_dir;
use crate::ipc::protocol::DaemonEvent;
use crate::ipc::DaemonClient;

/// Capacity of the per-connection event forward queue. If a client
/// can't keep up, the broadcast Receiver lags and the connection task
/// resubscribes (clients see a `Lagged` notice on the next event).
const EVENT_FORWARD_CAPACITY: usize = 256;

/// Bind the socket and accept connections forever. Returns `Err` only
/// if the bind itself fails; per-connection errors are logged and the
/// loop continues.
pub async fn serve(core: Arc<DaemonCore>, path: &Path) -> std::io::Result<()> {
    ensure_parent_dir(path)?;
    handle_stale_socket(path).await?;

    let listener = UnixListener::bind(path)?;
    info!("ferrosonicd listening on {}", path.display());

    loop {
        match listener.accept().await {
            Ok((stream, _addr)) => {
                let core = core.clone();
                tokio::spawn(async move {
                    if let Err(e) = handle_connection(core, stream).await {
                        warn!("Client connection ended with error: {}", e);
                    }
                });
            }
            Err(e) => {
                error!("Accept failed: {}", e);
                // Brief pause to avoid spinning on EMFILE etc.
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
        }
    }
}

/// If the socket file exists, probe for a live daemon.
/// - Connection succeeds → return `AddrInUse`.
/// - Connection fails (e.g., ECONNREFUSED) → unlink and continue.
async fn handle_stale_socket(path: &Path) -> std::io::Result<()> {
    if !path.exists() {
        return Ok(());
    }
    match UnixStream::connect(path).await {
        Ok(_) => Err(std::io::Error::new(
            std::io::ErrorKind::AddrInUse,
            format!("daemon already running at {}", path.display()),
        )),
        Err(_) => {
            debug!("Removing stale socket at {}", path.display());
            std::fs::remove_file(path)?;
            Ok(())
        }
    }
}

/// Drive one client connection to completion. Returns when either side
/// closes the socket or a frame error occurs.
async fn handle_connection(core: Arc<DaemonCore>, stream: UnixStream) -> Result<(), FrameError> {
    let (read_half, mut write_half) = stream.into_split();
    let mut reader = read_half;

    let dispatcher: Arc<dyn DaemonClient> = Arc::new(InProcessClient::new(core.clone()));

    // Subscribe to daemon events for this connection.
    let mut events: broadcast::Receiver<DaemonEvent> = dispatcher.subscribe();

    // Per-connection writer queue: both the request-handler tasks and
    // the event-forward branch push into this. mpsc serialises writes
    // on the single write_half — UnixStream is not Sync.
    let (writer_tx, mut writer_rx) = tokio::sync::mpsc::channel::<Frame>(EVENT_FORWARD_CAPACITY);

    // Writer task.
    let writer_task = tokio::spawn(async move {
        while let Some(frame) = writer_rx.recv().await {
            if let Err(e) = write_frame(&mut write_half, &frame).await {
                debug!("Per-conn writer ending: {}", e);
                break;
            }
        }
        let _ = write_half.shutdown().await;
    });

    // Event-forward task.
    let event_writer_tx = writer_tx.clone();
    let event_task = tokio::spawn(async move {
        loop {
            match events.recv().await {
                Ok(ev) => {
                    if event_writer_tx.send(Frame::Event(ev)).await.is_err() {
                        break;
                    }
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    warn!("Event subscriber lagged by {}; resyncing", n);
                    // Emit a notification frame so the client knows.
                    let frame = Frame::Event(DaemonEvent::Notification {
                        message: format!("Client lagged by {} events", n),
                        is_error: false,
                    });
                    if event_writer_tx.send(frame).await.is_err() {
                        break;
                    }
                }
                Err(broadcast::error::RecvError::Closed) => break,
            }
        }
    });

    // Read loop: dispatch requests, write responses.
    loop {
        let frame = match read_frame(&mut reader).await {
            Ok(f) => f,
            Err(FrameError::Closed) => {
                debug!("Client closed connection");
                break;
            }
            Err(e) => {
                warn!("Frame read error from client: {}", e);
                break;
            }
        };

        match frame {
            Frame::Request { id, req } => {
                let dispatcher = dispatcher.clone();
                let writer_tx = writer_tx.clone();
                tokio::spawn(async move {
                    let result = dispatcher.request(req).await;
                    let payload = result.map_err(|e| e.to_string());
                    let resp = Frame::Response { id, payload };
                    let _ = writer_tx.send(resp).await;
                });
            }
            Frame::Response { id, .. } => {
                warn!("Client sent a Response frame (id={}), ignoring", id);
            }
            Frame::Event(_) => {
                warn!("Client sent an Event frame, ignoring");
            }
        }
    }

    // Drop writer_tx so writer_task ends; event_task ends when its
    // own writer_tx clone is dropped (this holds the last reference
    // when this function returns).
    drop(writer_tx);
    let _ = event_task.await;
    let _ = writer_task.await;
    Ok(())
}
