//! Auto-spawn `ferrosonicd` when the TUI starts and no daemon is
//! reachable on the socket.

#![allow(dead_code)]

use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use tracing::{info, warn};

/// Locate the `ferrosonicd` binary. Tries:
/// 1. Sibling of the running executable (typical install layout from
///    a single `cargo install` or distro package).
/// 2. `$PATH` lookup.
fn locate_ferrosonicd() -> Option<PathBuf> {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let sibling = dir.join("ferrosonicd");
            if sibling.is_file() {
                return Some(sibling);
            }
        }
    }
    which_in_path("ferrosonicd")
}

fn which_in_path(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

/// Spawn `ferrosonicd` detached from the current terminal. The child
/// gets a fresh session via `setsid` so it survives `SIGHUP` when the
/// parent terminal closes. stdin/stdout/stderr are routed to
/// `/dev/null` so output doesn't bleed into the dying terminal.
///
/// Returns the spawned child's PID for logging. The parent does *not*
/// wait on the child — that's the whole point.
pub fn spawn_daemon() -> std::io::Result<u32> {
    let Some(bin) = locate_ferrosonicd() else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "ferrosonicd binary not found in $PATH or alongside ferrosonic",
        ));
    };

    info!("Auto-spawning daemon: {}", bin.display());

    let mut cmd = Command::new(&bin);
    cmd.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    // SAFETY: setsid is async-signal-safe and standard for daemonising
    // a child between fork and exec.
    unsafe {
        cmd.pre_exec(|| {
            if libc::setsid() == -1 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }

    let child = cmd.spawn()?;
    let pid = child.id();
    std::mem::forget(child); // do not wait, do not reap; let the daemon outlive us
    Ok(pid)
}

/// Spawn a daemon and wait up to `timeout` for its socket to come up.
pub async fn spawn_and_wait(
    socket: &Path,
    timeout: std::time::Duration,
) -> std::io::Result<()> {
    let pid = spawn_daemon()?;
    info!("Daemon spawned (pid {}); waiting for socket {}", pid, socket.display());
    match crate::ipc::path::wait_for_socket(socket, timeout).await {
        Ok(()) => {
            info!("Daemon socket ready");
            Ok(())
        }
        Err(e) => {
            warn!("Daemon spawned but socket did not come up: {}", e);
            Err(e)
        }
    }
}
