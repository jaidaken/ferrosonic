//! ferrosonicd binary smoke test: starts, binds socket, accepts Ping, exits cleanly.

use std::time::Duration;

use ferrosonic::ipc::client::DaemonClient;
use ferrosonic::ipc::protocol::{DaemonRequest, DaemonResponse};
use ferrosonic::ipc::SocketClient;
use tokio::time::sleep;

#[tokio::test]
async fn ferrosonicd_binary_boots_serves_socket_and_responds_to_ping() {
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

    let bin = assert_cmd::cargo::cargo_bin("ferrosonicd");
    let mut child = std::process::Command::new(&bin)
        .env("FERROSONIC_CONFIG_DIR", config_dir.path())
        .env("XDG_RUNTIME_DIR", runtime_dir.path())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("spawn ferrosonicd");

    for _ in 0..50 {
        if socket_path.exists() {
            break;
        }
        sleep(Duration::from_millis(100)).await;
    }

    let client = SocketClient::connect(&socket_path).await;
    let result = match &client {
        Ok(c) => c.request(DaemonRequest::Ping).await,
        Err(e) => panic!(
            "connect failed: {:?}; socket exists: {}",
            e,
            socket_path.exists()
        ),
    };

    let _ = child.kill();
    let _ = child.wait();

    let resp = result.expect("Ping request");
    assert!(
        matches!(resp, DaemonResponse::Pong),
        "expected Pong, got {:?}",
        resp
    );
}
