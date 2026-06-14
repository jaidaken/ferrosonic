//! update_server_config routes the password to the configured password_file
//! (non-empty path) instead of saving it inline. Kills the
//! `filter(|s| !s.is_empty())` -> `delete !` mutant, which drops the path and
//! never writes the file.

mod common;

use common::TestDaemon;
use ferrosonic::secret::Secret;
use serial_test::serial;

#[tokio::test]
#[serial]
async fn password_is_written_to_the_file_when_password_file_is_set() {
    let td = TestDaemon::new().await;
    let tmp = tempfile::tempdir().unwrap();
    let pf = tmp.path().join("pw");
    {
        let mut s = td.state.write().await;
        s.config.password_file = Some(pf.to_string_lossy().into_owned());
    }
    // Use the fake server URL so the post-update library refresh does not stall.
    let url = td.fake_subsonic.url();

    td.core
        .update_server_config(&url, "user", &Secret::from_string("hunter2".into()))
        .await
        .unwrap();

    assert!(pf.exists(), "the password file is written");
    assert_eq!(
        std::fs::read_to_string(&pf).unwrap().trim(),
        "hunter2",
        "the password file contains the password"
    );
}
