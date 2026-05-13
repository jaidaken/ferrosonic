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
use crate::ipc::frame::{
    read_frame_lenient_with_cap, write_frame, Frame, FrameError, FrameRead,
    MAX_REQUEST_FRAME_BYTES,
};
use crate::ipc::path::ensure_parent_dir;
use crate::ipc::protocol::DaemonEvent;
use crate::ipc::DaemonClient;

const EVENT_FORWARD_CAPACITY: usize = 256;

/// Reserve two slots so the snapshot pair is sent atomically or not at all; partial sends would leave the client desynced.
async fn try_send_resync(
    tx: &tokio::sync::mpsc::Sender<Frame>,
    core: &Arc<DaemonCore>,
) -> bool {
    let p1 = match tx.try_reserve() {
        Ok(p) => p,
        Err(_) => return false,
    };
    let p2 = match tx.try_reserve() {
        Ok(p) => p,
        Err(_) => {
            drop(p1);
            return false;
        }
    };
    let snap = core.snapshot().await;
    p1.send(Frame::Event(DaemonEvent::NowPlayingChanged(
        snap.now_playing.clone(),
    )));
    p2.send(Frame::Event(DaemonEvent::QueueChanged {
        queue: snap.queue.clone(),
        position: snap.queue_position,
    }));
    true
}

/// Mask password/secret values via serde_json round-trip; on parse failure returns a placeholder so a malformed body containing a password is never logged raw.
fn redact_secrets_in_body(body: &str) -> String {
    let mut val: serde_json::Value = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(_) => return "<unparseable; redacted>".to_string(),
    };
    redact_in_value(&mut val);
    serde_json::to_string(&val).unwrap_or_else(|_| "<reserialize-failed; redacted>".to_string())
}

fn redact_in_value(v: &mut serde_json::Value) {
    match v {
        serde_json::Value::Object(map) => {
            for (k, vv) in map.iter_mut() {
                let lower = k.to_ascii_lowercase();
                if lower.contains("password") || lower.contains("secret") {
                    *vv = serde_json::Value::String("***".to_string());
                } else {
                    redact_in_value(vv);
                }
            }
        }
        serde_json::Value::Array(arr) => {
            for it in arr.iter_mut() {
                redact_in_value(it);
            }
        }
        _ => {}
    }
}

/// `Err` only on bind failure; per-connection errors are logged.
pub async fn serve(core: Arc<DaemonCore>, path: &Path) -> std::io::Result<()> {
    ensure_parent_dir(path)?;
    let _lock = acquire_socket_lock(path)?;
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

/// Holds lock for daemon lifetime so two daemons cannot race past handle_stale_socket.
fn acquire_socket_lock(path: &Path) -> std::io::Result<std::fs::File> {
    use std::os::unix::io::AsRawFd;
    let lock_path = path.with_extension("lock");
    let file = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .open(&lock_path)?;
    let r = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
    if r != 0 {
        let err = std::io::Error::last_os_error();
        return Err(std::io::Error::new(
            std::io::ErrorKind::AddrInUse,
            format!(
                "daemon already running (lock {} held: {})",
                lock_path.display(),
                err
            ),
        ));
    }
    Ok(file)
}

/// Connect-probe to detect a stale socket; flock above already serialised access.
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
        let mut needs_resync = false;
        let mut last_resync_at: Option<std::time::Instant> = None;
        let mut retry = tokio::time::interval(std::time::Duration::from_millis(500));
        retry.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                ev_res = events.recv() => match ev_res {
                    Ok(ev) => match event_writer_tx.try_send(Frame::Event(ev)) {
                        Ok(()) => {
                            if needs_resync
                                && try_send_resync(&event_writer_tx, &event_core).await
                            {
                                needs_resync = false;
                                last_resync_at = Some(std::time::Instant::now());
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
                },
                _ = retry.tick() => {
                    if needs_resync {
                        let due = last_resync_at
                            .map(|t| t.elapsed() >= std::time::Duration::from_millis(500))
                            .unwrap_or(true);
                        if due
                            && try_send_resync(&event_writer_tx, &event_core).await
                        {
                            needs_resync = false;
                            last_resync_at = Some(std::time::Instant::now());
                        }
                    }
                }
            }
        }
    });

    loop {
        let read = match read_frame_lenient_with_cap(&mut reader, MAX_REQUEST_FRAME_BYTES).await {
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
