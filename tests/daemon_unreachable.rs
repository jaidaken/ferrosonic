//! 0.4.1 regression: TUI must exit with error when daemon is unreachable.

use assert_cmd::Command;
use predicates::{str::contains, Predicate};

#[test]
fn ferrosonic_exits_when_daemon_cannot_be_reached_or_spawned() {
    let isolated = tempfile::tempdir().expect("isolated bin dir");
    let config_dir = tempfile::tempdir().expect("config tempdir");
    let runtime_dir = tempfile::tempdir().expect("runtime tempdir");

    let original = assert_cmd::cargo::cargo_bin("ferrosonic");
    let target = isolated.path().join("ferrosonic");
    std::fs::copy(&original, &target).expect("copy ferrosonic to isolated dir");

    std::fs::write(
        config_dir.path().join("config.toml"),
        "BaseURL = \"http://127.0.0.1:1\"\nUsername = \"x\"\nPassword = \"x\"\nDaemon = true\n",
    )
    .unwrap();

    let output = Command::new(&target)
        .env("FERROSONIC_CONFIG_DIR", config_dir.path())
        .env("XDG_RUNTIME_DIR", runtime_dir.path())
        .env("PATH", "/nonexistent")
        .timeout(std::time::Duration::from_secs(15))
        .output()
        .expect("spawn isolated ferrosonic");

    assert!(
        !output.status.success(),
        "ferrosonic must exit non-zero when daemon is unreachable; got status {:?}",
        output.status
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        contains("could not reach ferrosonicd").eval(stderr.as_ref()),
        "stderr must explain the daemon failure; got:\n{}",
        stderr
    );
}
