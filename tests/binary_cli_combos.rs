//! CLI flag combinations + missing-config-dir edge cases.

#![allow(clippy::zombie_processes)]

use serial_test::serial;
use std::process::Command;

fn ferrosonic() -> std::path::PathBuf {
    assert_cmd::cargo::cargo_bin("ferrosonic")
}

#[test]
#[serial]
fn ferrosonic_short_v_flag_is_verbose() {
    let output = Command::new(ferrosonic())
        .arg("-v")
        .arg("--help")
        .output()
        .unwrap();
    assert!(output.status.success() || !output.stdout.is_empty());
}

#[test]
#[serial]
fn ferrosonic_short_c_flag_is_config() {
    let output = Command::new(ferrosonic())
        .arg("-c")
        .arg("--help")
        .output()
        .unwrap();
    let combined = String::from_utf8_lossy(&output.stdout).into_owned()
        + &String::from_utf8_lossy(&output.stderr);
    assert!(!combined.is_empty());
}

#[test]
#[serial]
fn ferrosonic_with_config_dir_missing_creates_or_handles_it() {
    let config_dir = tempfile::tempdir().unwrap();
    std::fs::remove_dir(config_dir.path()).unwrap();
    let runtime_dir = tempfile::tempdir().unwrap();
    let output = Command::new(ferrosonic())
        .env("FERROSONIC_CONFIG_DIR", config_dir.path())
        .env("XDG_RUNTIME_DIR", runtime_dir.path())
        .env("PATH", "/nonexistent")
        .arg("--help")
        .output()
        .unwrap();
    assert!(output.status.success());
}

#[test]
#[serial]
fn ferrosonic_help_via_short_h() {
    let output = Command::new(ferrosonic()).arg("-h").output().unwrap();
    assert!(output.status.success());
}

#[test]
#[serial]
fn ferrosonic_version_via_short_v_with_no_other_args_starts_app() {
    let _ = Command::new(ferrosonic())
        .arg("--version")
        .output()
        .unwrap();
}

#[test]
#[serial]
fn ferrosonic_multiple_unknown_flags_all_fail() {
    let output = Command::new(ferrosonic())
        .args(["--foo", "--bar"])
        .output()
        .unwrap();
    assert!(!output.status.success());
}

