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

    // Under a test runner: no setsid (stay in the test's group for a timeout
    // group-kill) + PR_SET_PDEATHSIG (die on parent exit, even via SIGKILL).
    let reap_with_parent = std::env::var_os("FERROSONIC_TEST_REAP_DAEMON").is_some()
        || std::env::var_os("NEXTEST").is_some();

    let mut cmd = Command::new(exe);
    cmd.arg("--daemon")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    // SAFETY: only async-signal-safe calls (setsid/prctl/getppid/raise) run here.
    unsafe {
        cmd.pre_exec(move || {
            if reap_with_parent {
                #[cfg(target_os = "linux")]
                {
                    if libc::prctl(
                        libc::PR_SET_PDEATHSIG,
                        libc::SIGKILL as libc::c_ulong,
                        0,
                        0,
                        0,
                    ) == -1
                    {
                        return Err(std::io::Error::last_os_error());
                    }
                    // Parent may have exited between fork and prctl; PDEATHSIG
                    // never fires then, so self-terminate if already reparented.
                    if libc::getppid() == 1 {
                        libc::raise(libc::SIGKILL);
                    }
                }
            } else if libc::setsid() == -1 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }

    let child = cmd.spawn()?;
    let pid = child.id();
    // Forget: don't reap. In production the daemon outlives us; under a test
    // runner PDEATHSIG / the group-kill reaps it when the test process exits.
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
