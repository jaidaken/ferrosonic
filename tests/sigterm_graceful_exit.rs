//! Real-process SIGTERM delivery to ferrosonicd.

#![allow(clippy::zombie_processes)]

mod common;
use std::time::Duration;

#[tokio::test]
async fn ferrosonicd_exits_cleanly_on_sigterm() {
    let config_dir = common::tempdir();
    let runtime_dir = common::tempdir();
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
        .expect("spawn ferrosonicd");

    for _ in 0..50 {
        if socket_path.exists() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(socket_path.exists(), "daemon failed to bind socket");

    let pid = child.id() as i32;
    let r = unsafe { libc::kill(pid, libc::SIGTERM) };
    assert_eq!(r, 0, "kill -TERM returned errno; libc::kill failed");

    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    let mut exited = None;
    while std::time::Instant::now() < deadline {
        if let Ok(Some(status)) = child.try_wait() {
            exited = Some(status);
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    if exited.is_none() {
        let _ = child.kill();
        let _ = child.wait();
        panic!("daemon did not exit within 5s of SIGTERM; signal handler likely not registered");
    }
    let status = exited.unwrap();
    assert!(
        status.success() || status.code().is_some(),
        "expected clean exit; got abnormal termination: {:?}",
        status
    );
}

#[tokio::test]
async fn ferrosonicd_exits_cleanly_on_sigint() {
    let config_dir = common::tempdir();
    let runtime_dir = common::tempdir();
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
        .unwrap();

    for _ in 0..50 {
        if socket_path.exists() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    let pid = child.id() as i32;
    let r = unsafe { libc::kill(pid, libc::SIGINT) };
    assert_eq!(r, 0);

    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    let mut exited = false;
    while std::time::Instant::now() < deadline {
        if matches!(child.try_wait(), Ok(Some(_))) {
            exited = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    if !exited {
        let _ = child.kill();
        let _ = child.wait();
        panic!("daemon did not exit on SIGINT within 5s");
    }
}
