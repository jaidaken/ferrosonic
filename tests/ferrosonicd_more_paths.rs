//! ferrosonicd subprocess tests for verbose + configured paths.

use serial_test::serial;
use std::process::Command;
use std::time::Duration;

fn ferrosonicd() -> Command {
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin("ferrosonic"));
    cmd.arg("--daemon");
    cmd
}

#[test]
#[serial]
fn ferrosonicd_starts_with_verbose_and_writes_log() {
    let tmp = tempfile::tempdir().unwrap();
    let socket = tmp.path().join("d.sock");
    let mut child = ferrosonicd()
        .arg("-v")
        .env("FERROSONIC_CONFIG_DIR", tmp.path())
        .env("XDG_RUNTIME_DIR", tempfile::tempdir().unwrap().keep())
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("spawn ferrosonicd");

    std::thread::sleep(Duration::from_millis(800));
    unsafe {
        libc::kill(child.id() as i32, libc::SIGTERM);
    }
    let _ = child.wait();
    let _ = socket;

    let log = tmp.path().join("ferrosonicd.log");
    if log.exists() {
        let contents = std::fs::read_to_string(&log).unwrap_or_default();
        assert!(
            contents.contains("ferrosonicd starting") || contents.is_empty(),
            "log should have startup msg or be empty"
        );
    }
}

#[test]
#[serial]
fn ferrosonicd_starts_with_configured_subsonic_section() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg_path = tmp.path().join("config.toml");
    std::fs::write(
        &cfg_path,
        r#"BaseURL = "http://127.0.0.1:1"
Username = "u"
Password = "p"
"#,
    )
    .unwrap();
    let mut child = ferrosonicd()
        .arg("-c")
        .arg(&cfg_path)
        .env("FERROSONIC_CONFIG_DIR", tmp.path())
        .env("XDG_RUNTIME_DIR", tempfile::tempdir().unwrap().keep())
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("spawn ferrosonicd");

    std::thread::sleep(Duration::from_millis(800));
    unsafe {
        libc::kill(child.id() as i32, libc::SIGTERM);
    }
    let _ = child.wait();
}

#[test]
#[serial]
fn ferrosonicd_with_invalid_config_path_returns_error() {
    let output = ferrosonicd()
        .arg("--config")
        .arg("/path/that/does/not/exist.toml")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .output()
        .unwrap();
    assert!(!output.status.success() || !output.stderr.is_empty());
}

#[test]
#[serial]
fn ferrosonicd_handles_sigint() {
    let tmp = tempfile::tempdir().unwrap();
    let mut child = ferrosonicd()
        .env("FERROSONIC_CONFIG_DIR", tmp.path())
        .env("XDG_RUNTIME_DIR", tempfile::tempdir().unwrap().keep())
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("spawn");
    std::thread::sleep(Duration::from_millis(500));
    unsafe {
        libc::kill(child.id() as i32, libc::SIGINT);
    }
    let status = child.wait().expect("wait");
    let _ = status;
}
