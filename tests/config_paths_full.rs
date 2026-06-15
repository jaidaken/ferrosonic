//! config/paths.rs: every getter.

mod common;
use ferrosonic::config::paths::{
    config_dir, config_file, ensure_config_dir, log_file, mpv_socket_path, queue_file, themes_dir,
};
use serial_test::serial;

#[test]
#[serial]
fn config_dir_uses_override_env_var_when_set() {
    let tmp = common::tempdir();
    std::env::set_var("FERROSONIC_CONFIG_DIR", tmp.path());
    let d = config_dir().expect("override yields Some");
    assert_eq!(d, tmp.path());
    std::env::remove_var("FERROSONIC_CONFIG_DIR");
}

#[test]
#[serial]
fn config_dir_falls_back_to_xdg_default_when_no_override() {
    std::env::remove_var("FERROSONIC_CONFIG_DIR");
    let d = config_dir();
    if let Some(d) = d {
        assert!(
            d.ends_with("ferrosonic"),
            "default config dir should end in 'ferrosonic'; got {}",
            d.display()
        );
    }
}

#[test]
#[serial]
fn config_file_appends_config_toml() {
    let tmp = common::tempdir();
    std::env::set_var("FERROSONIC_CONFIG_DIR", tmp.path());
    let f = config_file().expect("file path");
    assert!(f.ends_with("config.toml"));
    std::env::remove_var("FERROSONIC_CONFIG_DIR");
}

#[test]
#[serial]
fn themes_dir_appends_themes_segment() {
    let tmp = common::tempdir();
    std::env::set_var("FERROSONIC_CONFIG_DIR", tmp.path());
    let d = themes_dir().expect("themes dir");
    assert!(d.ends_with("themes"));
    std::env::remove_var("FERROSONIC_CONFIG_DIR");
}

#[test]
#[serial]
fn log_file_appends_ferrosonic_log() {
    let tmp = common::tempdir();
    std::env::set_var("FERROSONIC_CONFIG_DIR", tmp.path());
    let f = log_file().expect("log path");
    assert!(f.ends_with("ferrosonic.log"));
    std::env::remove_var("FERROSONIC_CONFIG_DIR");
}

#[test]
#[serial]
fn queue_file_appends_queue_json() {
    let tmp = common::tempdir();
    std::env::set_var("FERROSONIC_CONFIG_DIR", tmp.path());
    let f = queue_file().expect("queue path");
    assert!(f.ends_with("queue.json"));
    std::env::remove_var("FERROSONIC_CONFIG_DIR");
}

#[test]
fn mpv_socket_path_lives_in_temp_dir() {
    let p = mpv_socket_path();
    assert!(p.to_string_lossy().contains("ferrosonic-mpv.sock"));
}

#[test]
#[serial]
fn ensure_config_dir_creates_when_missing() {
    let tmp = common::tempdir();
    let target = tmp.path().join("nested-fresh");
    std::env::set_var("FERROSONIC_CONFIG_DIR", &target);
    assert!(!target.exists());
    let created = ensure_config_dir().expect("ensure");
    assert_eq!(created, target);
    assert!(target.is_dir());
    std::env::remove_var("FERROSONIC_CONFIG_DIR");
}

#[test]
#[serial]
fn ensure_config_dir_is_idempotent_when_already_exists() {
    let tmp = common::tempdir();
    std::env::set_var("FERROSONIC_CONFIG_DIR", tmp.path());
    let r1 = ensure_config_dir().unwrap();
    let r2 = ensure_config_dir().unwrap();
    assert_eq!(r1, r2);
    std::env::remove_var("FERROSONIC_CONFIG_DIR");
}
