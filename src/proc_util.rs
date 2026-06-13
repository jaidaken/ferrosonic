//! Process-lifecycle helpers shared by subprocess owners (mpv, cava).

use std::process::Command;

/// Make `cmd`'s child receive `SIGKILL` when this process dies, even on a
/// SIGKILL or crash where `Drop` never runs; without it an orphaned child
/// keeps holding resources (mpv the audio device, cava a PTY). Linux-only;
/// a no-op elsewhere.
pub fn set_die_with_parent(cmd: &mut Command) {
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::process::CommandExt;
        // SAFETY: the closure runs in the forked child before exec; prctl,
        // getppid and _exit are async-signal-safe.
        unsafe {
            cmd.pre_exec(|| {
                libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL);
                if libc::getppid() == 1 {
                    libc::_exit(1);
                }
                Ok(())
            });
        }
    }
    #[cfg(not(target_os = "linux"))]
    let _ = cmd;
}
