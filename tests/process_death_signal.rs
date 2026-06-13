//! set_die_with_parent: a child must be killed when its parent process dies
//! without cleanup. Guards the mpv-orphan leak (docs/SUBPROCESS-LEAK-BUG.md):
//! on SIGKILL/crash, Drop never runs, so the kernel has to do the killing.

use std::io::Read;
use std::os::unix::io::FromRawFd;
use std::process::Command;
use std::time::{Duration, Instant};

use ferrosonic::proc_util::set_die_with_parent;

/// True once `pid` is gone or a zombie (killed but not yet reaped).
fn dead_or_zombie(pid: i32) -> bool {
    match std::fs::read_to_string(format!("/proc/{pid}/stat")) {
        Err(_) => true,
        Ok(stat) => stat
            .rsplit(')')
            .next()
            .and_then(|rest| rest.split_whitespace().next())
            .map_or(true, |state| state == "Z"),
    }
}

#[test]
fn child_is_killed_when_its_parent_dies_without_cleanup() {
    let mut fds = [0i32; 2];
    assert_eq!(unsafe { libc::pipe(fds.as_mut_ptr()) }, 0, "pipe");
    let (read_fd, write_fd) = (fds[0], fds[1]);

    let pid = unsafe { libc::fork() };
    assert!(pid >= 0, "fork");
    if pid == 0 {
        // Child: spawn a guarded long-lived `sleep`, report its pid, then exit
        // with no cleanup. The kernel must SIGKILL the sleep via PDEATHSIG.
        unsafe { libc::close(read_fd) };
        let mut cmd = Command::new("sleep");
        cmd.arg("30");
        set_die_with_parent(&mut cmd);
        let sleep_pid = cmd.spawn().map(|c| c.id()).unwrap_or(0);
        let bytes = sleep_pid.to_le_bytes();
        unsafe { libc::write(write_fd, bytes.as_ptr().cast(), bytes.len()) };
        unsafe { libc::_exit(0) };
    }

    unsafe { libc::close(write_fd) };
    let mut f = unsafe { std::fs::File::from_raw_fd(read_fd) };
    let mut buf = [0u8; 4];
    f.read_exact(&mut buf).expect("read sleep pid");
    let sleep_pid = u32::from_le_bytes(buf) as i32;
    assert!(sleep_pid > 0, "child failed to spawn sleep");

    let mut status = 0;
    unsafe { libc::waitpid(pid, &mut status, 0) };

    let deadline = Instant::now() + Duration::from_secs(2);
    let mut gone = false;
    while Instant::now() < deadline {
        if dead_or_zombie(sleep_pid) {
            gone = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    if !gone {
        unsafe { libc::kill(sleep_pid, libc::SIGKILL) };
    }
    assert!(
        gone,
        "guarded sleep {sleep_pid} must be killed when its parent dies"
    );
}
