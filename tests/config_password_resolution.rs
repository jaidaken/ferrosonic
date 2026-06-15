//! config/mod.rs: password resolution priority + save/load round-trip.

mod common;
use ferrosonic::config::Config;
use serial_test::serial;
use std::io::Write;

#[test]
#[serial]
fn ferrosonic_password_env_wins_over_inline_and_file() {
    let tmp = common::tempdir();
    let pwfile = tmp.path().join("pw.txt");
    std::fs::write(&pwfile, "file-password").unwrap();
    let config_path = tmp.path().join("config.toml");
    std::fs::write(
        &config_path,
        format!(
            r#"BaseURL = "https://x"
Username = "u"
Password = "inline-password"
PasswordFile = "{}"
"#,
            pwfile.display()
        ),
    )
    .unwrap();
    std::env::set_var("FERROSONIC_PASSWORD", "env-wins");
    let c = Config::load_from_file(&config_path).unwrap();
    std::env::remove_var("FERROSONIC_PASSWORD");
    assert_eq!(c.password_str(), "env-wins");
}

#[test]
#[serial]
fn password_file_wins_over_inline_when_no_env() {
    let tmp = common::tempdir();
    let pwfile = tmp.path().join("pw.txt");
    std::fs::write(&pwfile, "file-password\n").unwrap();
    let config_path = tmp.path().join("config.toml");
    std::fs::write(
        &config_path,
        format!(
            r#"BaseURL = "https://x"
Username = "u"
Password = "inline"
PasswordFile = "{}"
"#,
            pwfile.display()
        ),
    )
    .unwrap();
    std::env::remove_var("FERROSONIC_PASSWORD");
    let c = Config::load_from_file(&config_path).unwrap();
    assert_eq!(
        c.password_str(),
        "file-password",
        "trailing whitespace must be trimmed"
    );
}

#[test]
#[serial]
fn password_file_unreadable_clears_inline_to_avoid_stale_credentials() {
    let tmp = common::tempdir();
    let config_path = tmp.path().join("config.toml");
    std::fs::write(
        &config_path,
        r#"BaseURL = "https://x"
Username = "u"
Password = "fallback-inline"
PasswordFile = "/nonexistent-file"
"#,
    )
    .unwrap();
    std::env::remove_var("FERROSONIC_PASSWORD");
    let c = Config::load_from_file(&config_path).unwrap();
    assert_eq!(c.password_str(), "");
}

#[test]
#[serial]
fn password_file_tilde_path_expands_to_home() {
    let tmp = common::tempdir();
    std::env::set_var("HOME", tmp.path());
    let pwfile = tmp.path().join("pw.txt");
    std::fs::write(&pwfile, "tilde-resolved").unwrap();
    let config_path = tmp.path().join("config.toml");
    std::fs::write(
        &config_path,
        r#"BaseURL = "https://x"
Username = "u"
PasswordFile = "~/pw.txt"
"#,
    )
    .unwrap();
    std::env::remove_var("FERROSONIC_PASSWORD");
    let c = Config::load_from_file(&config_path).unwrap();
    assert_eq!(c.password_str(), "tilde-resolved");
}

#[test]
#[serial]
fn load_from_file_missing_path_returns_error() {
    let r = Config::load_from_file(std::path::Path::new("/no/such/file/exists/here"));
    assert!(r.is_err());
}

#[test]
#[serial]
fn load_from_file_with_invalid_toml_returns_parse_error() {
    let tmp = common::tempdir();
    let p = tmp.path().join("broken.toml");
    let mut f = std::fs::File::create(&p).unwrap();
    f.write_all(b"this is not valid toml [[[").unwrap();
    let r = Config::load_from_file(&p);
    assert!(r.is_err());
}

#[test]
#[serial]
fn save_to_file_creates_parent_directories() {
    let tmp = common::tempdir();
    let target = tmp.path().join("nested/deep/config.toml");
    let c = Config {
        base_url: "https://example.com".into(),
        ..Default::default()
    };
    c.save_to_file(&target).unwrap();
    assert!(target.is_file());
    let loaded = Config::load_from_file(&target).unwrap();
    assert_eq!(loaded.base_url, "https://example.com");
}

#[test]
#[serial]
fn load_default_returns_default_when_no_file() {
    let tmp = common::tempdir();
    std::env::set_var("FERROSONIC_CONFIG_DIR", tmp.path());
    let c = Config::load_default().expect("default config when no file");
    assert_eq!(c.base_url, "");
    std::env::remove_var("FERROSONIC_CONFIG_DIR");
}

#[test]
#[serial]
fn load_default_reads_existing_file() {
    let tmp = common::tempdir();
    std::env::set_var("FERROSONIC_CONFIG_DIR", tmp.path());
    let p = tmp.path().join("config.toml");
    std::fs::write(
        &p,
        r#"BaseURL = "https://configured"
Username = "user"
Password = "pw"
"#,
    )
    .unwrap();
    std::env::remove_var("FERROSONIC_PASSWORD");
    let c = Config::load_default().unwrap();
    assert_eq!(c.base_url, "https://configured");
    std::env::remove_var("FERROSONIC_CONFIG_DIR");
}

#[test]
#[serial]
fn is_configured_false_for_empty_fields() {
    let c = Config::default();
    assert!(!c.is_configured());
}

#[test]
#[serial]
fn is_configured_true_when_all_three_fields_set() {
    let c = Config {
        base_url: "x".into(),
        username: "u".into(),
        password: "p".into(),
        ..Default::default()
    };
    assert!(c.is_configured());
    assert_eq!(c.password_str(), "p");
}

#[test]
#[serial]
fn is_configured_false_when_password_empty() {
    let c = Config {
        base_url: "x".into(),
        username: "u".into(),
        ..Default::default()
    };
    assert!(!c.is_configured());
}

#[test]
#[serial]
fn resolved_password_does_not_leak_plaintext_through_debug() {
    let tmp = common::tempdir();
    let p = tmp.path().join("c.toml");
    std::fs::write(
        &p,
        r#"BaseURL = "https://x"
Username = "u"
Password = "super-secret-PW-123"
"#,
    )
    .unwrap();
    std::env::remove_var("FERROSONIC_PASSWORD");
    let c = Config::load_from_file(&p).unwrap();
    assert_eq!(c.password_str(), "super-secret-PW-123");
    let d = format!("{:?}", c);
    assert!(
        !d.contains("super-secret-PW-123"),
        "Config Debug must not contain plaintext password: {}",
        d
    );
}

#[test]
#[serial]
fn empty_ferrosonic_password_env_does_not_override() {
    let tmp = common::tempdir();
    let p = tmp.path().join("c.toml");
    std::fs::write(
        &p,
        r#"BaseURL = ""
Username = ""
Password = "kept"
"#,
    )
    .unwrap();
    std::env::set_var("FERROSONIC_PASSWORD", "");
    let c = Config::load_from_file(&p).unwrap();
    std::env::remove_var("FERROSONIC_PASSWORD");
    assert_eq!(c.password_str(), "kept");
}
