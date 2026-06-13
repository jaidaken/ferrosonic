//! Regression: a `DaemonRequest::Shutdown` over IPC must make the daemon
//! process exit, not merely broadcast the event and keep listening.

use std::time::Duration;

use ferrosonic::ipc::client::DaemonClient;
use ferrosonic::ipc::protocol::DaemonRequest;
use ferrosonic::ipc::SocketClient;

#[tokio::test]
async fn ipc_shutdown_request_terminates_the_daemon_process() {
    let config_dir = tempfile::tempdir().expect("config tempdir");
    let runtime_dir = tempfile::tempdir().expect("runtime tempdir");
    let socket_dir = runtime_dir.path().join("ferrosonic");
    std::fs::create_dir_all(&socket_dir).unwrap();
    let socket_path = socket_dir.join("ferrosonicd.sock");

    std::fs::write(
        config_dir.path().join("config.toml"),
        "BaseURL = \"\"\nUsername = \"x\"\nPassword = \"x\"\nDaemon = true\n",
    )
    .unwrap();

    let bin = assert_cmd::cargo::cargo_bin("ferrosonic");
    let mut child = std::process::Command::new(&bin)
        .arg("--daemon")
        .env("FERROSONIC_CONFIG_DIR", config_dir.path())
        .env("XDG_RUNTIME_DIR", runtime_dir.path())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("spawn ferrosonic --daemon");

    for _ in 0..50 {
        if socket_path.exists() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(socket_path.exists(), "daemon failed to bind socket");

    let client = SocketClient::connect(&socket_path)
        .await
        .expect("connect to daemon socket");
    client
        .request(DaemonRequest::Shutdown)
        .await
        .expect("Shutdown request");

    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    let mut exited = None;
    while std::time::Instant::now() < deadline {
        if let Ok(Some(status)) = child.try_wait() {
            exited = Some(status);
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    let _ = child.kill();
    let _ = child.wait();

    assert!(
        exited.is_some(),
        "daemon kept running after IPC Shutdown; it must stop accepting and exit"
    );
    assert!(
        exited.unwrap().success(),
        "daemon should exit cleanly after IPC Shutdown"
    );
}
