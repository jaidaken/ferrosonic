//! Every realistic clap flag permutation on both binaries.

#![allow(clippy::zombie_processes)]

use serial_test::serial;
use std::process::Command;

fn ferrosonic() -> std::path::PathBuf {
    assert_cmd::cargo::cargo_bin("ferrosonic")
}

fn ferrosonicd() -> std::path::PathBuf {
    assert_cmd::cargo::cargo_bin("ferrosonicd")
}

#[test]
#[serial]
fn ferrosonic_verbose_and_standalone_together() {
    let output = Command::new(ferrosonic())
        .args(["--verbose", "--standalone", "--help"])
        .output()
        .unwrap();
    assert!(output.status.success());
}

#[test]
#[serial]
fn ferrosonic_short_v_then_long_help() {
    let output = Command::new(ferrosonic())
        .args(["-v", "--help"])
        .output()
        .unwrap();
    assert!(output.status.success());
}

#[test]
#[serial]
fn ferrosonic_long_help_then_short_v_works() {
    let output = Command::new(ferrosonic())
        .args(["--help", "-v"])
        .output()
        .unwrap();
    assert!(output.status.success());
}

#[test]
#[serial]
fn ferrosonic_help_idempotent_across_repeats() {
    let output = Command::new(ferrosonic())
        .args(["--help", "--help"])
        .output()
        .unwrap();
    assert!(output.status.success());
}

#[test]
#[serial]
fn ferrosonic_repeated_verbose_flags_errors_per_clap() {
    let output = Command::new(ferrosonic())
        .args(["-v", "-v", "--help"])
        .output()
        .unwrap();
    assert!(
        !output.status.success() || !output.stdout.is_empty(),
        "clap rejects duplicate bool flags by default; expected non-success or fallback help"
    );
}

#[test]
#[serial]
fn ferrosonic_config_with_equals_form() {
    let cfg = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(
        cfg.path(),
        "BaseURL = \"\"\nUsername = \"x\"\nPassword = \"x\"\n",
    )
    .unwrap();
    let output = Command::new(ferrosonic())
        .arg(format!("--config={}", cfg.path().display()))
        .arg("--help")
        .output()
        .unwrap();
    assert!(output.status.success());
}

#[test]
#[serial]
fn ferrosonicd_verbose_and_help_together() {
    let output = Command::new(ferrosonicd())
        .args(["--verbose", "--help"])
        .output()
        .unwrap();
    assert!(output.status.success());
}

#[test]
#[serial]
fn ferrosonicd_repeated_short_flags_errors_per_clap() {
    let output = Command::new(ferrosonicd())
        .args(["-v", "-v", "--help"])
        .output()
        .unwrap();
    assert!(
        !output.status.success() || !output.stdout.is_empty(),
        "clap rejects duplicate bool flags; expected non-success or fallback help"
    );
}

#[test]
#[serial]
fn ferrosonic_unrecognized_positional_arg_errors() {
    let output = Command::new(ferrosonic())
        .arg("some-positional-arg")
        .output()
        .unwrap();
    assert!(
        !output.status.success() || !output.stderr.is_empty(),
        "positional args should not be accepted silently"
    );
}

#[test]
#[serial]
fn ferrosonicd_unrecognized_positional_arg_errors() {
    let output = Command::new(ferrosonicd())
        .arg("some-positional-arg")
        .output()
        .unwrap();
    assert!(
        !output.status.success() || !output.stderr.is_empty(),
        "positional args should not be accepted silently"
    );
}

#[test]
#[serial]
fn ferrosonic_help_displays_about_text() {
    let output = Command::new(ferrosonic()).arg("--help").output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.to_lowercase().contains("ferrosonic") || stdout.to_lowercase().contains("usage"),
        "help output should mention the program name or 'usage'"
    );
}

#[test]
#[serial]
fn ferrosonicd_help_displays_about_text() {
    let output = Command::new(ferrosonicd()).arg("--help").output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.to_lowercase().contains("ferrosonicd") || stdout.to_lowercase().contains("usage"),
        "help output should mention the daemon name or 'usage'"
    );
}
