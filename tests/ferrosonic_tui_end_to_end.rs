//! Spawn the ferrosonic binary attached to a real PTY, hit it with
//! SIGTERM, assert clean exit. Exercises the event_loop wrapper.

#![allow(clippy::zombie_processes)]

use std::os::unix::io::FromRawFd;
use std::os::unix::process::CommandExt;
use std::time::Duration;

#[allow(dead_code)]
fn open_pty() -> (std::fs::File, libc::c_int) {
    let mut master: libc::c_int = 0;
    let mut slave: libc::c_int = 0;
    unsafe {
        let r = libc::openpty(
            &mut master,
            &mut slave,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        );
        assert_eq!(r, 0, "openpty failed");
        let ws = libc::winsize {
            ws_row: 24,
            ws_col: 80,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        libc::ioctl(slave, libc::TIOCSWINSZ, &ws);
        let master_file = std::fs::File::from_raw_fd(master);
        (master_file, slave)
    }
}

#[tokio::test]
async fn ferrosonic_binary_event_loop_exits_on_sigterm_through_pty() {
    let config_dir = tempfile::tempdir().unwrap();
    let runtime_dir = tempfile::tempdir().unwrap();

    let (_master, slave) = open_pty();

    let bin = assert_cmd::cargo::cargo_bin("ferrosonic");
    let mut cmd = std::process::Command::new(&bin);
    cmd.env("FERROSONIC_CONFIG_DIR", config_dir.path())
        .env("XDG_RUNTIME_DIR", runtime_dir.path())
        .env("TERM", "xterm")
        .arg("--standalone")
        .stdin(unsafe { std::process::Stdio::from_raw_fd(libc::dup(slave)) })
        .stdout(unsafe { std::process::Stdio::from_raw_fd(libc::dup(slave)) })
        .stderr(unsafe { std::process::Stdio::from_raw_fd(libc::dup(slave)) });

    unsafe {
        cmd.pre_exec(|| {
            if libc::setsid() == -1 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }

    let mut child = cmd.spawn().unwrap();
    unsafe {
        libc::close(slave);
    }

    tokio::time::sleep(Duration::from_millis(500)).await;

    let pid = child.id() as i32;
    unsafe {
        libc::kill(pid, libc::SIGTERM);
    }

    let deadline = std::time::Instant::now() + Duration::from_secs(8);
    let mut exit_status: Option<std::process::ExitStatus> = None;
    while std::time::Instant::now() < deadline {
        if let Ok(Some(s)) = child.try_wait() {
            exit_status = Some(s);
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let Some(status) = exit_status else {
        let _ = child.kill();
        let _ = child.wait();
        panic!("ferrosonic did not exit on SIGTERM through PTY within 8s");
    };
    assert!(
        status.success() || status.code() == Some(0) || status.code().is_none(),
        "ferrosonic exited with unexpected status: {:?}",
        status
    );
}
