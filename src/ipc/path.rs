//! Socket path resolution.
//!
//! Order: `$FERROSONIC_SOCK` →
//! `$XDG_RUNTIME_DIR/ferrosonic/ferrosonicd.sock` →
//! `/tmp/ferrosonic-{uid}/ferrosonicd.sock`. AF_UNIX caps at 108 bytes.


use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use tokio::net::UnixStream;

const SOCKET_FILENAME: &str = "ferrosonicd.sock";
const SUBDIR: &str = "ferrosonic";

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

/// chmod 0700 only when we created the directory; XDG_RUNTIME_DIR is
/// already restricted and /tmp must not be touched.
pub fn ensure_parent_dir(path: &std::path::Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let Some(parent) = path.parent() else {
        return Ok(());
    };
    if parent.exists() {
        return Ok(());
    }
    std::fs::create_dir_all(parent)?;
    let mut perm = match std::fs::metadata(parent) {
        Ok(m) => m.permissions(),
        Err(_) => return Ok(()),
    };
    perm.set_mode(0o700);
    let _ = std::fs::set_permissions(parent, perm);
    Ok(())
}

/// Poll until connect succeeds or `timeout` elapses.
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
