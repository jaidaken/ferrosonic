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
        .test_server_connection("http://127.0.0.1:1", "u", &"p".into())
        .await;
    assert!(!ok);
    assert!(
        msg.contains("Connection failed"),
        "expected 'Connection failed' prefix, got: {}",
        msg
    );
}

#[tokio::test]
#[serial]
async fn http_error_status_is_reported_as_http_status_not_parse() {
    use ferrosonic::error::SubsonicError;
    use ferrosonic::subsonic::SubsonicClient;

    let td = TestDaemon::new().await;
    td.fake_subsonic
        .expect_http_status("getStarred2", 503)
        .await;
    let client = SubsonicClient::new(&td.fake_subsonic.url(), "u", &"p".into()).expect("client");
    let err = client
        .get_starred_songs()
        .await
        .expect_err("503 must be an error");
    assert!(
        matches!(err, SubsonicError::HttpStatus { status: 503 }),
        "a non-2xx response must surface as HttpStatus, not a parse error: {err:?}"
    );
}

#[tokio::test]
#[serial]
async fn transport_error_does_not_leak_auth_token_or_salt() {
    use ferrosonic::secret::Secret;
    use ferrosonic::subsonic::SubsonicClient;

    // Port 1 is closed, so the request fails at transport level. `reqwest`
    // would otherwise append the full URL (with the `t` token and `s` salt) to
    // its error Display; the client must strip it before it can be logged.
    let client =
        SubsonicClient::new("http://127.0.0.1:1", "u", &Secret::from("p")).expect("client");
    let err = client
        .get_starred_songs()
        .await
        .expect_err("connection to port 1 must fail");
    let msg = err.to_string();
    assert!(
        !msg.contains("127.0.0.1:1") && !msg.contains("rest/getStarred2"),
        "transport error must have the URL (and its auth params) stripped: {msg}"
    );
}
