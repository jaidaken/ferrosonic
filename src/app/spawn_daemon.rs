//! Auto-spawn the daemon when the TUI starts and none is reachable on the
//! socket. The daemon is this same binary re-exec'd with `--daemon`.

use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Command, Stdio};

use tracing::{info, warn};

/// Spawn the daemon by re-running the current binary with `--daemon`, detached
/// via `setsid` so it survives SIGHUP when the parent terminal closes. The
/// parent never reaps it; the daemon outlives the TUI.
pub fn spawn_daemon() -> std::io::Result<u32> {
    let exe = std::env::current_exe()?;
    spawn_daemon_exe(&exe)
}

/// Test seam: spawn a specific binary as the daemon. Production passes
/// `current_exe()`; tests pass the real `ferrosonic` binary.
pub fn spawn_daemon_exe(exe: &Path) -> std::io::Result<u32> {
    info!("Auto-spawning daemon: {} --daemon", exe.display());

    let mut cmd = Command::new(exe);
    cmd.arg("--daemon")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    // SAFETY: setsid is async-signal-safe.
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
    // Forget: don't reap, the daemon outlives us.
    std::mem::forget(child);
    Ok(pid)
}

/// Spawn the daemon detached and wait until its socket accepts connections.
pub async fn spawn_and_wait(socket: &Path, timeout: std::time::Duration) -> std::io::Result<()> {
    let pid = spawn_daemon()?;
    info!(
        "Daemon spawned (pid {}); waiting for socket {}",
        pid,
        socket.display()
    );
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
