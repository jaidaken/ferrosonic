//! OS-keychain credential storage: save routes to the keychain by default,
//! falls back to an owner-only inline write when no keychain is reachable,
//! and the resolution chain reads the password back from the keychain.

mod common;

use std::sync::Arc;

use ferrosonic::config::Config;
use ferrosonic::ipc::PasswordStorage;
use ferrosonic::secret::Secret;
use ferrosonic::secret_store::{self, InMemoryKeyStore};

use common::TestDaemon;

#[tokio::test]
#[serial_test::serial]
async fn save_stores_password_in_keychain_and_writes_no_plaintext() {
    let td = TestDaemon::new().await;
    let url = td.fake_subsonic.url();

    let storage = td
        .core
        .update_server_config(&url, "alice", &Secret::from("hunter2"))
        .await
        .expect("save succeeds");
    assert_eq!(storage, PasswordStorage::Keyring);

    let written = std::fs::read_to_string(td.config_dir.path().join("config.toml")).unwrap();
    assert!(
        written.contains("PasswordKeyring = true"),
        "keyring marker must be persisted:\n{written}"
    );
    assert!(
        !written.contains("hunter2"),
        "the plaintext password must not be written to config:\n{written}"
    );

    let stored = secret_store::retrieve(&url, "alice").unwrap();
    assert_eq!(stored.as_ref().map(Secret::reveal), Some("hunter2"));
}

#[tokio::test]
#[serial_test::serial]
async fn save_falls_back_to_inline_owner_only_when_keychain_unavailable() {
    let td = TestDaemon::new().await;
    // Replace the working fixture store with one that reports no keychain.
    secret_store::install_test_store(Arc::new(InMemoryKeyStore::unavailable()));
    let url = td.fake_subsonic.url();

    let storage = td
        .core
        .update_server_config(&url, "bob", &Secret::from("fallbackpw"))
        .await
        .expect("save succeeds via fallback");
    assert_eq!(storage, PasswordStorage::Inline);

    let path = td.config_dir.path().join("config.toml");
    let written = std::fs::read_to_string(&path).unwrap();
    assert!(
        written.contains("fallbackpw"),
        "with no keychain the password is written inline:\n{written}"
    );
    assert!(
        !written.contains("PasswordKeyring = true"),
        "keyring marker must not be set on the inline fallback:\n{written}"
    );

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(
            mode, 0o600,
            "inline-secret config must be owner-only, got {mode:o}"
        );
    }
}

#[test]
#[serial_test::serial]
fn resolution_reads_password_from_keychain_when_marker_set() {
    secret_store::install_test_store(Arc::new(InMemoryKeyStore::new()));
    secret_store::store("https://nav.example", "carol", &Secret::from("kcpass")).unwrap();

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(
        &path,
        "BaseURL = \"https://nav.example\"\nUsername = \"carol\"\nPasswordKeyring = true\n",
    )
    .unwrap();

    let cfg = Config::load_from_file(&path).unwrap();
    assert_eq!(cfg.password.reveal(), "kcpass");
    secret_store::clear_test_store();
}

#[test]
#[serial_test::serial]
fn resolution_clears_password_when_marker_set_but_keychain_empty() {
    secret_store::install_test_store(Arc::new(InMemoryKeyStore::new()));

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(
        &path,
        "BaseURL = \"https://nav.example\"\nUsername = \"nobody\"\nPasswordKeyring = true\n",
    )
    .unwrap();

    let cfg = Config::load_from_file(&path).unwrap();
    assert!(
        cfg.password.is_empty(),
        "a set marker with no keychain entry must not leave a stale credential"
    );
    secret_store::clear_test_store();
}
