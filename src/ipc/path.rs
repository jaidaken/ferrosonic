//! Socket path resolution.
//!
//! Daemon and client both call `socket_path()` to find the per-user
//! IPC endpoint. Resolution order:
//!
//! 1. `$FERROSONIC_SOCK` — explicit override; honoured verbatim. Used
//!    by tests and unusual deployments.
//! 2. `$XDG_RUNTIME_DIR/ferrosonic/ferrosonicd.sock` — the standard
//!    location on a normal NixOS / systemd-user setup. Created with
//!    mode 0700 on the parent directory; the socket file inherits the
//!    process umask.
//! 3. `/tmp/ferrosonic-{uid}/ferrosonicd.sock` — fallback when
//!    `XDG_RUNTIME_DIR` is unset (Docker, minimal containers, broken
//!    sessions). The numeric uid keeps it per-user without colliding.
//!
//! Path lengths matter: AF_UNIX paths max out at 108 bytes on Linux.
//! `$XDG_RUNTIME_DIR` is typically `/run/user/<uid>`, well under that.

#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use tokio::net::UnixStream;

/// Filename inside the IPC directory.
const SOCKET_FILENAME: &str = "ferrosonicd.sock";

/// Subdirectory under the runtime root.
const SUBDIR: &str = "ferrosonic";

/// Resolve the daemon's socket path.
pub fn socket_path() -> PathBuf {
    if let Ok(custom) = std::env::var("FERROSONIC_SOCK") {
        return PathBuf::from(custom);
    }
    if let Ok(rt) = std::env::var("XDG_RUNTIME_DIR") {
        let mut p = PathBuf::from(rt);
        p.push(SUBDIR);
        p.push(SOCKET_FILENAME);
        return p;
    }
    let uid = unsafe { libc::getuid() };
    let mut p = PathBuf::from("/tmp");
    p.push(format!("ferrosonic-{}", uid));
    p.push(SOCKET_FILENAME);
    p
}

/// Ensure the socket's parent directory exists with sane permissions.
/// Daemon calls this before binding; client never calls it.
///
/// Only chmods the directory when we just created it. The standard
/// `XDG_RUNTIME_DIR` path is already mode 0700; `/tmp` is 1777 and
/// chmodding it would fail and is undesirable. The per-uid subdir
/// is what we care about: when *we* mkdir it, set 0700.
pub fn ensure_parent_dir(path: &std::path::Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let Some(parent) = path.parent() else {
        return Ok(());
    };
    if parent.exists() {
        return Ok(());
    }
    std::fs::create_dir_all(parent)?;
    // Best-effort chmod on the directory we just created. If this
    // fails (e.g., parent of parent is sticky), ignore — the socket
    // file itself is the security boundary.
    let mut perm = match std::fs::metadata(parent) {
        Ok(m) => m.permissions(),
        Err(_) => return Ok(()),
    };
    perm.set_mode(0o700);
    let _ = std::fs::set_permissions(parent, perm);
    Ok(())
}

/// Poll the socket path until a connection succeeds or `timeout`
/// elapses. Used by the TUI right after auto-spawning `ferrosonicd` to
/// wait for it to be ready. Returns `Ok(())` on first successful
/// connect; `Err` if the deadline passes.
pub async fn wait_for_socket(path: &Path, timeout: Duration) -> std::io::Result<()> {
    let deadline = Instant::now() + timeout;
    let mut delay = Duration::from_millis(25);
    loop {
        if UnixStream::connect(path).await.is_ok() {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                format!("daemon socket {} did not become ready", path.display()),
            ));
        }
        tokio::time::sleep(delay).await;
        delay = (delay * 2).min(Duration::from_millis(200));
    }
}
