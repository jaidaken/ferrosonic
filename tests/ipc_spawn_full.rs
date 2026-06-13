//! app/spawn_daemon.rs: spawn + socket-wait error path.

use std::time::Duration;

use serial_test::serial;

#[tokio::test]
#[serial]
async fn wait_for_socket_errs_when_a_dummy_daemon_never_binds() {
    // /usr/bin/env is one of two FHS paths guaranteed on NixOS; `--daemon` is
    // rejected so it exits at once and never binds the socket.
    let dummy = std::path::Path::new("/usr/bin/env");
    let sock = std::env::temp_dir().join(format!("ferrosonic-nodaemon-{}.sock", std::process::id()));
    let _ = std::fs::remove_file(&sock);

    let pid = ferrosonic::app::spawn_daemon::spawn_daemon_exe(dummy).expect("spawn dummy");
    let r = ferrosonic::ipc::path::wait_for_socket(&sock, Duration::from_millis(200)).await;
    unsafe {
        libc::kill(pid as i32, libc::SIGTERM);
    }
    assert!(r.is_err(), "the socket must not come up for a dummy daemon");
}
