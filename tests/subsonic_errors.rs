//! SubsonicClient error handling against fake error responses.

mod common;

use common::TestDaemon;
use serial_test::serial;

#[tokio::test]
#[serial]
async fn api_error_response_propagates_as_subsonic_error() {
    let td = TestDaemon::new().await;
    td.fake_subsonic
        .expect_error("getArtists", 40, "Wrong username or password")
        .await;

    td.core.refresh_artists().await;
    let s = td.state.read().await;
    assert!(
        s.library.artists.is_empty(),
        "error response must not populate state"
    );
}

#[tokio::test]
#[serial]
async fn http_500_does_not_crash_refresh() {
    let td = TestDaemon::new().await;
    td.fake_subsonic
        .expect_http_status("getStarred2", 500)
        .await;
    td.core.refresh_starred().await;
    let s = td.state.read().await;
    assert!(s.library.starred_songs.is_empty());
}

#[tokio::test]
#[serial]
async fn http_404_does_not_crash_search() {
    let td = TestDaemon::new().await;
    td.fake_subsonic.expect_http_status("search3", 404).await;
    let results = td.core.search("anything", 1, 1, 1).await;
    assert!(results.artist.is_empty());
    assert!(results.album.is_empty());
    assert!(results.song.is_empty());
}

#[tokio::test]
#[serial]
async fn connection_refused_returns_error_from_test_server() {
    let td = TestDaemon::new().await;
    let (ok, msg) = td
        .core
        .test_server_connection("http://127.0.0.1:1", "u", "p")
        .await;
    assert!(!ok);
    assert!(
        msg.contains("Connection failed"),
        "expected 'Connection failed' prefix, got: {}",
        msg
    );
}
