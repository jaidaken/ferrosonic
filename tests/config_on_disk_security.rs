//! Credential-on-disk handling: the password is omitted from the config file
//! when a separate password_file is configured, and the password file itself is
//! written atomically at mode 0600.

mod common;
use ferrosonic::config::{write_password_file_atomic, Config};
use ferrosonic::secret::Secret;

#[test]
fn password_is_not_written_to_config_when_a_password_file_is_set() {
    // The `||`->`&&` and dropped-`!` mutants in as_on_disk would write the
    // plaintext password into the config despite the password_file being set.
    let mut c = Config::new();
    c.base_url = "https://example.com".into();
    c.username = "u".into();
    c.password = Secret::from_string("hunter2".to_string());
    c.password_file = Some("/home/u/.config/ferrosonic/pw".into());

    let dir = common::tempdir();
    let path = dir.path().join("config.toml");
    c.save_to_file(&path).expect("save");

    let contents = std::fs::read_to_string(&path).expect("read config");
    assert!(
        !contents.contains("hunter2"),
        "password must not be written to the config when a password_file is set:\n{contents}"
    );
}

#[test]
fn password_file_is_written_with_the_secret_and_mode_0600() {
    use std::os::unix::fs::PermissionsExt;

    // Parent dir is missing on purpose: the writer must create it, then write the
    // secret atomically at 0600.
    let dir = common::tempdir();
    let path = dir.path().join("sub").join("pw");
    write_password_file_atomic(
        path.to_str().expect("utf8 path"),
        &Secret::from_string("hunter2".to_string()),
    )
    .expect("write password file");

    let contents = std::fs::read_to_string(&path).expect("read pw file");
    assert_eq!(contents, "hunter2\n", "password file holds the secret");

    let mode = std::fs::metadata(&path).expect("stat").permissions().mode();
    assert_eq!(
        mode & 0o777,
        0o600,
        "password file must be owner-only (0600)"
    );
}
