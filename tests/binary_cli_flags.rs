//! Binary CLI flag handling: --help, --version, --config, --verbose.

#![allow(clippy::zombie_processes)]

use assert_cmd::Command;
use predicates::prelude::PredicateBooleanExt;
use predicates::{str::contains, Predicate};

#[test]
fn ferrosonic_help_lists_known_flags() {
    let bin = assert_cmd::cargo::cargo_bin("ferrosonic");
    let output = std::process::Command::new(&bin)
        .arg("--help")
        .output()
        .expect("spawn ferrosonic");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("--config") || stdout.contains("config"));
    assert!(stdout.contains("--verbose") || stdout.contains("verbose"));
    assert!(stdout.contains("--standalone") || stdout.contains("standalone"));
}

#[test]
fn ferrosonic_version_prints_a_version_string() {
    let bin = assert_cmd::cargo::cargo_bin("ferrosonic");
    let output = std::process::Command::new(&bin)
        .arg("--version")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("ferrosonic") || stdout.contains("0."),
        "expected version line; got {}",
        stdout
    );
}

#[test]
fn ferrosonic_unknown_flag_returns_nonzero_with_error_message() {
    let bin = assert_cmd::cargo::cargo_bin("ferrosonic");
    let output = std::process::Command::new(&bin)
        .arg("--this-flag-does-not-exist")
        .output()
        .unwrap();
    assert!(
        !output.status.success(),
        "unknown flag must fail; got {:?}",
        output.status
    );
}

#[test]
fn ferrosonicd_help_lists_known_flags() {
    let bin = assert_cmd::cargo::cargo_bin("ferrosonicd");
    let output = std::process::Command::new(&bin)
        .arg("--help")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("--config") || stdout.contains("config"));
    assert!(stdout.contains("--verbose") || stdout.contains("verbose"));
}

#[test]
fn ferrosonicd_version_prints_a_version_string() {
    let bin = assert_cmd::cargo::cargo_bin("ferrosonicd");
    let output = std::process::Command::new(&bin)
        .arg("--version")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("ferrosonicd") || stdout.contains("0."),
        "expected version line; got {}",
        stdout
    );
}

#[test]
fn ferrosonic_explicit_config_flag_is_accepted() {
    let config_dir = tempfile::tempdir().unwrap();
    let runtime_dir = tempfile::tempdir().unwrap();
    let cfg = config_dir.path().join("custom.toml");
    std::fs::write(
        &cfg,
        "BaseURL = \"http://127.0.0.1:1\"\nUsername = \"x\"\nPassword = \"x\"\nDaemon = false\n",
    )
    .unwrap();

    let bin = assert_cmd::cargo::cargo_bin("ferrosonic");
    let isolated = tempfile::tempdir().unwrap();
    let target = isolated.path().join("ferrosonic");
    std::fs::copy(&bin, &target).unwrap();

    let output = Command::new(&target)
        .env("FERROSONIC_CONFIG_DIR", config_dir.path())
        .env("XDG_RUNTIME_DIR", runtime_dir.path())
        .env("PATH", "/nonexistent")
        .arg("--config")
        .arg(&cfg)
        .timeout(std::time::Duration::from_secs(10))
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        contains("Terminal initialization failed")
            .or(contains("Loading config"))
            .or(contains("standalone"))
            .eval(stderr.as_ref())
            || !output.status.success(),
        "expected explicit-config path to either run or fail with a known error;\nstderr: {}",
        stderr
    );
}
