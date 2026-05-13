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
use crate::ipc::frame::{read_frame_lenient, write_frame, Frame, FrameError, FrameRead};
use crate::ipc::path::ensure_parent_dir;
use crate::ipc::protocol::DaemonEvent;
use crate::ipc::DaemonClient;

const EVENT_FORWARD_CAPACITY: usize = 256;

/// Push a snapshot-derived NowPlayingChanged + QueueChanged pair when
/// the per-conn writer has room. Returns true on success; on partial
/// success or full channel, caller keeps `needs_resync` set.
async fn try_send_resync(
    tx: &tokio::sync::mpsc::Sender<Frame>,
    core: &Arc<DaemonCore>,
) -> bool {
    use tokio::sync::mpsc::error::TrySendError;
    let snap = core.snapshot().await;
    let now = Frame::Event(DaemonEvent::NowPlayingChanged(snap.now_playing.clone()));
    let queue = Frame::Event(DaemonEvent::QueueChanged {
        queue: snap.queue.clone(),
        position: snap.queue_position,
    });
    if let Err(TrySendError::Closed(_)) = tx.try_send(now) {
        return false;
    }
    matches!(tx.try_send(queue), Ok(()))
}

/// Replace plaintext password/secret values in a JSON-ish body string
/// with `***` before logging. Conservative: any key containing
/// "password" or "secret" gets its quoted-string value redacted.
fn redact_secrets_in_body(body: &str) -> String {
    let mut out = String::with_capacity(body.len());
    let bytes = body.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let rest = &body[i..];
        let maybe = ["password", "Password", "secret", "Secret"]
            .iter()
            .find_map(|key| rest.find(key).map(|p| (p, key.len())));
        let Some((rel_pos, key_len)) = maybe else {
            out.push_str(rest);
            break;
        };
        out.push_str(&rest[..rel_pos + key_len]);
        let after = &rest[rel_pos + key_len..];
        if let Some(colon) = after.find(':') {
            out.push_str(&after[..=colon]);
            let val = &after[colon + 1..];
            let trimmed = val.trim_start();
            let leading = val.len() - trimmed.len();
            out.push_str(&val[..leading]);
            if trimmed.starts_with('"') {
                if let Some(end) = trimmed[1..].find('"') {
                    out.push_str("\"***\"");
                    i += rel_pos + key_len + colon + 1 + leading + 1 + end + 1;
                    continue;
                }
            }
        }
        i += rel_pos + key_len;
    }
    out
}

/// `Err` only on bind failure; per-connection errors are logged.
pub async fn serve(core: Arc<DaemonCore>, path: &Path) -> std::io::Result<()> {
    ensure_parent_dir(path)?;
    handle_stale_socket(path).await?;

    let listener = UnixListener::bind(path)?;
    info!("ferrosonicd listening on {}", path.display());

    loop {
        tokio::select! {
            biased;
            _ = core.shutdown_signal() => {
                info!("shutdown signalled; IPC accept loop exiting");
                return Ok(());
            }
            accepted = listener.accept() => match accepted {
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
    let event_core = core.clone();
    let event_task = tokio::spawn(async move {
        use tokio::sync::mpsc::error::TrySendError;
        // try_send so a frozen TUI cannot block the broadcast pump.
        // On Full or Lagged we set needs_resync, then deliver a single
        // snapshot pair the next time the channel has room.
        let mut needs_resync = false;
        loop {
            match events.recv().await {
                Ok(ev) => match event_writer_tx.try_send(Frame::Event(ev)) {
                    Ok(()) => {
                        if needs_resync {
                            needs_resync = !try_send_resync(&event_writer_tx, &event_core).await;
                        }
                    }
                    Err(TrySendError::Full(_)) => {
                        needs_resync = true;
                    }
                    Err(TrySendError::Closed(_)) => break,
                },
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    warn!("Event subscriber lagged by {}; will resync when room", n);
                    needs_resync = true;
                }
                Err(broadcast::error::RecvError::Closed) => break,
            }
        }
    });

    loop {
        let read = match read_frame_lenient(&mut reader).await {
            Ok(r) => r,
            Err(FrameError::Closed) => {
                debug!("Client closed connection");
                break;
            }
            Err(e) => {
                warn!("Frame read error from client: {}", e);
                break;
            }
        };

        match read {
            FrameRead::Ok(Frame::Request { id, req }) => {
                // Await inline so a single connection's requests are
                // served in arrival order. Concurrent multi-client
                // load is still handled by separate connection tasks.
                let result = dispatcher.request(req).await;
                let payload = result.map_err(|e| e.to_string());
                let resp = Frame::Response { id, payload };
                let _ = writer_tx.send(resp).await;
            }
            FrameRead::Ok(Frame::Response { id, .. }) => {
                warn!("Client sent a Response frame (id={}), ignoring", id);
            }
            FrameRead::Ok(Frame::Event(_)) => {
                warn!("Client sent an Event frame, ignoring");
            }
            FrameRead::UnknownRequest { id, body } => {
                // Bodies may carry password fields if an UpdateServerConfig
                // payload fails to deserialize for any reason; scrub
                // before logging.
                let redacted = redact_secrets_in_body(&body);
                warn!(
                    "Unknown request variant from client (id={}); replying with Err: {}",
                    id, redacted
                );
                let resp = Frame::Response {
                    id,
                    payload: Err(format!("unknown request variant: {}", redacted)),
                };
                let _ = writer_tx.send(resp).await;
            }
            FrameRead::UnknownResponse { id, .. } => {
                warn!("Client sent a Response frame (id={}), ignoring", id);
            }
            FrameRead::UnknownEvent { .. } => {
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
