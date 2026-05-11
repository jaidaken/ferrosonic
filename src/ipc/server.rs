//! Daemon-side IPC server: one task per connected client.

#![allow(dead_code)]

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

const EVENT_FORWARD_CAPACITY: usize = 256;

/// `Err` only on bind failure; per-connection errors are logged.
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
                // Backoff: avoid spinning on EMFILE.
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
        }
    }
}

/// Connect-and-fail probes for a live daemon at the socket path;
/// success means another daemon owns it, failure means it's stale.
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

async fn handle_connection(core: Arc<DaemonCore>, stream: UnixStream) -> Result<(), FrameError> {
    let (read_half, mut write_half) = stream.into_split();
    let mut reader = read_half;

    let dispatcher: Arc<dyn DaemonClient> = Arc::new(InProcessClient::new(core.clone()));
    let mut events: broadcast::Receiver<DaemonEvent> = dispatcher.subscribe();

    // mpsc serialises writes on the single write_half (UnixStream is !Sync).
    let (writer_tx, mut writer_rx) = tokio::sync::mpsc::channel::<Frame>(EVENT_FORWARD_CAPACITY);

    let writer_task = tokio::spawn(async move {
        while let Some(frame) = writer_rx.recv().await {
            if let Err(e) = write_frame(&mut write_half, &frame).await {
                debug!("Per-conn writer ending: {}", e);
                break;
            }
        }
        let _ = write_half.shutdown().await;
    });

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

    // Drop last writer_tx so writer_task ends.
    drop(writer_tx);
    let _ = event_task.await;
    let _ = writer_task.await;
    Ok(())
}
