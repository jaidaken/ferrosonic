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
        .update_server_config(&new_subsonic.url(), "newuser", "newpass")
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
        .test_server_connection(&td.fake_subsonic.url(), "u", "p")
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
        .test_server_connection("http://127.0.0.1:1", "u", "p")
        .await;
    assert!(!ok, "unreachable URL must return false");
    assert!(
        msg.starts_with("Connection failed"),
        "message should explain failure; got: {}",
        msg
    );
}
