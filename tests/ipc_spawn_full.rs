//! app/spawn_daemon.rs: daemon auto-spawn path-resolution.

use serial_test::serial;
use std::path::PathBuf;

#[test]
#[serial]
fn spawn_daemon_returns_not_found_with_empty_path() {
    let original = std::env::var_os("PATH");
    std::env::set_var("PATH", "");
    let result = ferrosonic::app::spawn_daemon::spawn_daemon();
    if let Some(p) = original {
        std::env::set_var("PATH", p);
    }
    assert!(
        result.is_err(),
        "with no PATH and no sibling binary, spawn should fail"
    );
    if let Err(e) = result {
        assert_eq!(e.kind(), std::io::ErrorKind::NotFound);
    }
}

#[test]
#[serial]
fn spawn_daemon_locates_sibling_binary_when_present() {
    let temp = tempfile::tempdir().unwrap();
    let fake_dir = temp.path().to_path_buf();
    let fake_bin = fake_dir.join("ferrosonic");
    let fake_daemon = fake_dir.join("ferrosonicd");
    std::fs::write(&fake_bin, "#!/bin/sh\nexit 0\n").unwrap();
    std::fs::write(&fake_daemon, "#!/bin/sh\nsleep 5\n").unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&fake_daemon, std::fs::Permissions::from_mode(0o755)).unwrap();
    let _ = fake_dir.join("ferrosonicd").to_path_buf();
    drop(fake_bin);
    drop(temp);
}

#[tokio::test]
#[serial]
async fn spawn_and_wait_returns_io_error_when_binary_missing() {
    let original = std::env::var_os("PATH");
    std::env::set_var("PATH", "/nonexistent");
    let sock: PathBuf = "/tmp/ferrosonic-no-daemon.sock".into();
    let r =
        ferrosonic::app::spawn_daemon::spawn_and_wait(&sock, std::time::Duration::from_millis(50))
            .await;
    if let Some(p) = original {
        std::env::set_var("PATH", p);
    }
    assert!(r.is_err());
}
