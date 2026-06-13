//! app/spawn_daemon.rs: the re-exec spawn brings up a real daemon that serves.

use std::time::Duration;

use ferrosonic::ipc::path::{socket_path, wait_for_socket};
use ferrosonic::ipc::{DaemonClient, DaemonRequest, DaemonResponse, SocketClient};
use serial_test::serial;

#[tokio::test]
#[serial]
async fn spawn_daemon_exe_starts_a_real_daemon_that_serves() {
    let exe = assert_cmd::cargo::cargo_bin("ferrosonic");
    assert!(exe.is_file(), "ferrosonic binary must exist");

    // Isolate the daemon's socket, config, and logs.
    let rt = tempfile::tempdir().unwrap();
    let cfg = tempfile::tempdir().unwrap();
    let prev_rt = std::env::var_os("XDG_RUNTIME_DIR");
    let prev_cfg = std::env::var_os("FERROSONIC_CONFIG_DIR");
    std::env::set_var("XDG_RUNTIME_DIR", rt.path());
    std::env::set_var("FERROSONIC_CONFIG_DIR", cfg.path());

    let socket = socket_path();
    let pid = ferrosonic::app::spawn_daemon::spawn_daemon_exe(&exe).expect("spawn daemon");
    let up = wait_for_socket(&socket, Duration::from_secs(5)).await;

    let mut pinged = false;
    if up.is_ok() {
        if let Ok(client) = SocketClient::connect(&socket).await {
            if let Ok(resp) = client.request(DaemonRequest::Ping).await {
                pinged = matches!(resp, DaemonResponse::Pong);
            }
        }
    }

    unsafe {
        libc::kill(pid as i32, libc::SIGTERM);
    }
    match prev_rt {
        Some(v) => std::env::set_var("XDG_RUNTIME_DIR", v),
        None => std::env::remove_var("XDG_RUNTIME_DIR"),
    }
    match prev_cfg {
        Some(v) => std::env::set_var("FERROSONIC_CONFIG_DIR", v),
        None => std::env::remove_var("FERROSONIC_CONFIG_DIR"),
    }

    assert!(up.is_ok(), "the re-exec'd daemon must bring up its socket");
    assert!(pinged, "the daemon must answer Ping with Pong");
}
