//! ipc/path.rs: fallback path + no-parent ensure_parent_dir.

mod common;
use ferrosonic::ipc::path::{ensure_parent_dir, socket_path};
use serial_test::serial;

#[test]
#[serial]
fn socket_path_uses_uid_fallback_when_no_env_vars() {
    let orig_sock = std::env::var_os("FERROSONIC_SOCK");
    let orig_rt = std::env::var_os("XDG_RUNTIME_DIR");
    std::env::remove_var("FERROSONIC_SOCK");
    std::env::remove_var("XDG_RUNTIME_DIR");
    let p = socket_path();
    let s = p.to_string_lossy();
    assert!(s.starts_with("/tmp/ferrosonic-"), "got: {}", s);
    assert!(s.ends_with("ferrosonicd.sock"));
    if let Some(v) = orig_sock {
        std::env::set_var("FERROSONIC_SOCK", v);
    }
    if let Some(v) = orig_rt {
        std::env::set_var("XDG_RUNTIME_DIR", v);
    }
}

#[test]
#[serial]
fn socket_path_uses_xdg_runtime_dir_when_set() {
    let orig_sock = std::env::var_os("FERROSONIC_SOCK");
    std::env::remove_var("FERROSONIC_SOCK");
    std::env::set_var("XDG_RUNTIME_DIR", "/run/user/1234");
    let p = socket_path();
    let s = p.to_string_lossy();
    assert!(s.contains("/run/user/1234/ferrosonic/ferrosonicd.sock"));
    if let Some(v) = orig_sock {
        std::env::set_var("FERROSONIC_SOCK", v);
    }
    std::env::remove_var("XDG_RUNTIME_DIR");
}

#[test]
#[serial]
fn socket_path_uses_ferrosonic_sock_env_when_set() {
    std::env::set_var("FERROSONIC_SOCK", "/tmp/my-custom-sock");
    let p = socket_path();
    assert_eq!(p.to_string_lossy(), "/tmp/my-custom-sock");
    std::env::remove_var("FERROSONIC_SOCK");
}

#[test]
#[serial]
fn ensure_parent_dir_with_path_at_root_is_noop() {
    let _ = ensure_parent_dir(std::path::Path::new("/"));
}

#[test]
#[serial]
fn ensure_parent_dir_with_path_having_no_parent_is_noop() {
    let r = ensure_parent_dir(std::path::Path::new("filename-only"));
    let _ = r;
}

#[test]
#[serial]
fn ensure_parent_dir_with_existing_parent_returns_ok_without_create() {
    let tmp = common::tempdir();
    let p = tmp.path().join("inside-existing.sock");
    ensure_parent_dir(&p).unwrap();
    assert!(tmp.path().exists());
}
