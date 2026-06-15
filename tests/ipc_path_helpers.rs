//! ipc/path: socket_path resolution + ensure_parent_dir + wait_for_socket.

mod common;
use std::time::Duration;

use ferrosonic::ipc::path::{ensure_parent_dir, socket_path, wait_for_socket};
use serial_test::serial;

#[test]
#[serial]
fn ferrosonic_sock_env_overrides_everything() {
    std::env::set_var("FERROSONIC_SOCK", "/tmp/custom-ferrosonic.sock");
    let path = socket_path();
    assert_eq!(path.to_string_lossy(), "/tmp/custom-ferrosonic.sock");
    std::env::remove_var("FERROSONIC_SOCK");
}

#[test]
#[serial]
fn xdg_runtime_dir_is_second_priority() {
    std::env::remove_var("FERROSONIC_SOCK");
    let dir = common::tempdir();
    std::env::set_var("XDG_RUNTIME_DIR", dir.path());
    let path = socket_path();
    assert!(path.starts_with(dir.path()));
    assert!(path.to_string_lossy().ends_with("ferrosonicd.sock"));
    std::env::remove_var("XDG_RUNTIME_DIR");
}

#[test]
#[serial]
fn ensure_parent_dir_creates_missing_directory_with_0700() {
    use std::os::unix::fs::PermissionsExt;
    let dir = common::tempdir();
    let sock = dir.path().join("subdir").join("ferrosonicd.sock");
    ensure_parent_dir(&sock).expect("ensure_parent_dir succeeds");
    let meta = std::fs::metadata(sock.parent().unwrap()).unwrap();
    let mode = meta.permissions().mode() & 0o777;
    assert_eq!(mode, 0o700, "parent dir should be chmod 0700 on creation");
}

#[test]
#[serial]
fn ensure_parent_dir_existing_directory_is_noop() {
    let dir = common::tempdir();
    let sock = dir.path().join("ferrosonicd.sock");
    ensure_parent_dir(&sock).expect("existing parent");
    ensure_parent_dir(&sock).expect("idempotent");
}

#[tokio::test]
#[serial]
async fn wait_for_socket_times_out_when_no_listener() {
    let dir = common::tempdir();
    let path = dir.path().join("never.sock");
    let r = wait_for_socket(&path, Duration::from_millis(150)).await;
    assert!(r.is_err(), "wait must timeout when no listener");
}

#[tokio::test]
#[serial]
async fn wait_for_socket_succeeds_once_listener_appears() {
    use tokio::net::UnixListener;
    let dir = common::tempdir();
    let path = dir.path().join("ready.sock");
    let listener = UnixListener::bind(&path).unwrap();
    let r = wait_for_socket(&path, Duration::from_millis(500)).await;
    assert!(
        r.is_ok(),
        "wait should connect immediately to a bound listener"
    );
    drop(listener);
}
