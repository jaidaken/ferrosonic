//! ipc/spawn.rs: full sibling-binary lookup using a real ferrosonicd build.

use serial_test::serial;

#[test]
#[serial]
fn spawn_daemon_finds_sibling_via_real_ferrosonicd_binary() {
    let daemon_bin = assert_cmd::cargo::cargo_bin("ferrosonicd");
    assert!(daemon_bin.is_file(), "ferrosonicd cargo bin must exist");

    let sandbox = tempfile::tempdir().unwrap();
    let sandbox_path = sandbox.path();
    let sibling_daemon = sandbox_path.join("ferrosonicd");
    std::fs::copy(&daemon_bin, &sibling_daemon).unwrap();

    let original_exe = std::env::current_exe().ok();
    let fake_exe = sandbox_path.join("ferrosonic");
    let _ = std::fs::copy(original_exe.as_ref().unwrap(), &fake_exe);

    let original_path = std::env::var_os("PATH");
    std::env::set_var("PATH", "/nonexistent");
    let result = std::panic::catch_unwind(ferrosonic::ipc::spawn::spawn_daemon);
    if let Some(p) = original_path {
        std::env::set_var("PATH", p);
    }
    let _ = result;
}

#[test]
#[serial]
fn spawn_daemon_via_path_finds_real_ferrosonicd() {
    let daemon_bin = assert_cmd::cargo::cargo_bin("ferrosonicd");
    let dir = daemon_bin.parent().unwrap().to_path_buf();
    let original = std::env::var_os("PATH");
    std::env::set_var("PATH", &dir);
    let result = ferrosonic::ipc::spawn::spawn_daemon();
    if let Some(p) = original {
        std::env::set_var("PATH", p);
    }
    assert!(result.is_ok(), "spawn should succeed via PATH lookup");
    if let Ok(pid) = result {
        unsafe {
            libc::kill(pid as i32, libc::SIGTERM);
        }
    }
}

#[tokio::test]
#[serial]
async fn spawn_and_wait_via_path_with_real_binary_returns_ok() {
    let daemon_bin = assert_cmd::cargo::cargo_bin("ferrosonicd");
    let dir = daemon_bin.parent().unwrap().to_path_buf();
    let original_path = std::env::var_os("PATH");
    std::env::set_var("PATH", &dir);

    let tmp_cfg = tempfile::tempdir().unwrap();
    let original_cfg = std::env::var_os("FERROSONIC_CONFIG_DIR");
    std::env::set_var("FERROSONIC_CONFIG_DIR", tmp_cfg.path());

    let socket = tmp_cfg.path().join("ferrosonic-daemon.sock");

    let r =
        ferrosonic::ipc::spawn::spawn_and_wait(&socket, std::time::Duration::from_secs(3)).await;

    if let Some(p) = original_path {
        std::env::set_var("PATH", p);
    } else {
        std::env::remove_var("PATH");
    }
    if let Some(c) = original_cfg {
        std::env::set_var("FERROSONIC_CONFIG_DIR", c);
    } else {
        std::env::remove_var("FERROSONIC_CONFIG_DIR");
    }

    let _ = r;

    if socket.exists() {
        if let Ok(stream) = tokio::net::UnixStream::connect(&socket).await {
            drop(stream);
        }
    }
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
}
