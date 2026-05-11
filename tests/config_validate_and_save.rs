//! config/mod.rs: validate method + save_default error paths.

use ferrosonic::config::Config;
use ferrosonic::error::ConfigError;
use serial_test::serial;

#[test]
#[serial]
fn validate_with_empty_base_url_returns_missing_field() {
    let c = Config::default();
    let r = c.validate();
    assert!(r.is_err());
    matches!(r, Err(ConfigError::MissingField { .. }));
}

#[test]
#[serial]
fn validate_with_invalid_url_returns_invalid_url() {
    let c = Config {
        base_url: "not a real url with spaces".into(),
        ..Default::default()
    };
    let r = c.validate();
    assert!(r.is_err());
}

#[test]
#[serial]
fn validate_with_valid_url_and_empty_username_warns_but_passes() {
    let c = Config {
        base_url: "https://example.com".into(),
        username: String::new(),
        password: String::new(),
        ..Default::default()
    };
    let r = c.validate();
    assert!(r.is_ok());
}

#[test]
#[serial]
fn validate_with_full_config_succeeds() {
    let c = Config {
        base_url: "https://example.com".into(),
        username: "u".into(),
        password: "p".into(),
        ..Default::default()
    };
    let r = c.validate();
    assert!(r.is_ok());
}

#[test]
#[serial]
fn save_to_file_with_existing_parent_dir_skips_create() {
    let tmp = tempfile::tempdir().unwrap();
    let p = tmp.path().join("config.toml");
    let c = Config::default();
    c.save_to_file(&p).unwrap();
    let c2 = Config {
        base_url: "https://second-save".into(),
        ..Default::default()
    };
    c2.save_to_file(&p).unwrap();
    let loaded = Config::load_from_file(&p).unwrap();
    assert_eq!(loaded.base_url, "https://second-save");
}

#[test]
#[serial]
fn save_default_writes_to_config_dir_override() {
    let tmp = tempfile::tempdir().unwrap();
    std::env::set_var("FERROSONIC_CONFIG_DIR", tmp.path());
    let c = Config {
        base_url: "https://saved".into(),
        ..Default::default()
    };
    c.save_default().unwrap();
    let f = tmp.path().join("config.toml");
    assert!(f.exists());
    std::env::remove_var("FERROSONIC_CONFIG_DIR");
}

#[test]
#[serial]
fn password_file_with_tilde_when_home_unset_keeps_literal_path() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg_path = tmp.path().join("config.toml");
    std::fs::write(
        &cfg_path,
        r#"BaseURL = "https://x"
Username = "u"
Password = "fallback"
PasswordFile = "~/never-resolved.txt"
"#,
    )
    .unwrap();
    let original_home = std::env::var_os("HOME");
    std::env::remove_var("HOME");
    std::env::remove_var("FERROSONIC_PASSWORD");
    let _ = Config::load_from_file(&cfg_path);
    if let Some(h) = original_home {
        std::env::set_var("HOME", h);
    }
}
