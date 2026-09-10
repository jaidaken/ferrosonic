//! `update_server_config` + `test_server_connection`.

mod common;

use common::TestDaemon;
use serial_test::serial;

#[tokio::test]
#[serial]
async fn update_server_config_swaps_subsonic_client_and_refreshes_library() {
    let td = TestDaemon::new().await;
    let new_subsonic = common::FakeSubsonic::start().await;
    new_subsonic.expect_starred_with(&["NewStar"]).await;
    new_subsonic.expect_artists(&["NewArtist"]).await;
    new_subsonic.expect_playlists().await;

    td.core
        .update_server_config(&new_subsonic.url(), "newuser", &"newpass".into())
        .await
        .expect("update succeeds");

    let s = td.state.read().await;
    assert_eq!(s.config.base_url, new_subsonic.url());
    assert_eq!(s.config.username, "newuser");
    assert_eq!(s.library.starred_songs.len(), 1);
    assert_eq!(s.library.artists.len(), 1);
}

#[tokio::test]
#[serial]
async fn test_server_connection_returns_ok_for_reachable_subsonic() {
    let td = TestDaemon::new().await;
    td.fake_subsonic.expect_ping().await;

    let (ok, msg) = td
        .core
        .test_server_connection(&td.fake_subsonic.url(), "u", &"p".into())
        .await;
    assert!(ok, "expected ok=true, got message: {}", msg);
    assert_eq!(msg, "Connection OK");
}

#[tokio::test]
#[serial]
async fn test_server_connection_returns_false_for_bad_url() {
    let td = TestDaemon::new().await;
    let (ok, msg) = td
        .core
        .test_server_connection("http://127.0.0.1:1", "u", &"p".into())
        .await;
    assert!(!ok, "unreachable URL must return false");
    assert!(
        msg.starts_with("Connection failed"),
        "message should explain failure; got: {}",
        msg
    );
}

#[tokio::test]
#[serial]
async fn update_server_config_persist_failure_leaves_prior_config_intact() {
    let td = TestDaemon::new().await;
    let before_url = td.state.read().await.config.base_url.clone();

    // Point the config directory at a path whose parent is a regular file, so
    // the atomic save's create_dir_all fails after credentials are staged.
    let blocker = td.config_dir.path().join("blocker");
    std::fs::write(&blocker, b"not a directory").unwrap();
    std::env::set_var("FERROSONIC_CONFIG_DIR", blocker.join("nested"));

    let result = td
        .core
        .update_server_config("https://new.example", "new", &"pw".into())
        .await;

    std::env::set_var("FERROSONIC_CONFIG_DIR", td.config_dir.path());

    assert!(result.is_err(), "a failed persist must surface an error");
    let s = td.state.read().await;
    assert_eq!(
        s.config.base_url, before_url,
        "the prior config must survive a failed persist"
    );
    assert_eq!(s.config.username, "test");
}

#[tokio::test]
#[serial]
async fn snapshot_and_config_changed_scrub_every_secret_source() {
    use ferrosonic::config::PasswordEval;
    use ferrosonic::ipc::protocol::DaemonEvent;

    let td = TestDaemon::new().await;
    {
        let mut s = td.state.write().await;
        s.config.password = "inline-secret".into();
        s.config.password_file = Some("/tmp/ferrosonic-pw".into());
        s.config.password_eval = Some(PasswordEval::Shell("printf hunter2".into()));
        s.config.password_keyring = true;
    }

    let snap = td.core.snapshot().await;
    assert!(snap.config.password.is_empty(), "password scrubbed");
    assert!(
        snap.config.password_file.is_none(),
        "password_file scrubbed"
    );
    assert!(
        snap.config.password_eval.is_none(),
        "password_eval (may embed a secret) scrubbed"
    );
    assert!(!snap.config.password_keyring, "keyring marker scrubbed");

    let mut rx = td.core.event_tx.subscribe();
    td.core.set_scrobble(true).await.expect("persist setting");

    let mut seen = None;
    for _ in 0..5 {
        match tokio::time::timeout(std::time::Duration::from_secs(1), rx.recv()).await {
            Ok(Ok(DaemonEvent::ConfigChanged(cfg))) => {
                seen = Some(cfg);
                break;
            }
            Ok(Ok(_)) => continue,
            _ => break,
        }
    }
    let cfg = seen.expect("ConfigChanged emitted");
    assert!(cfg.password.is_empty());
    assert!(cfg.password_file.is_none());
    assert!(
        cfg.password_eval.is_none(),
        "PasswordEval must not be broadcast to clients"
    );
    assert!(!cfg.password_keyring);
}
