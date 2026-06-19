//! Direct SubsonicClient endpoint tests against fake_subsonic.

mod common;

use common::FakeSubsonic;
use ferrosonic::error::SubsonicError;
use ferrosonic::subsonic::client::SubsonicClient;
use serial_test::serial;

async fn build_client(fake: &FakeSubsonic) -> SubsonicClient {
    SubsonicClient::new(&fake.url(), "test", &"test".into()).unwrap()
}

#[tokio::test]
#[serial]
async fn ping_succeeds_on_ok_response() {
    let fake = FakeSubsonic::start().await;
    fake.expect_ping().await;
    let c = build_client(&fake).await;
    c.ping().await.expect("ping ok");
}

#[tokio::test]
#[serial]
async fn ping_returns_error_on_failed_response() {
    let fake = FakeSubsonic::start().await;
    fake.expect_error("ping", 40, "auth").await;
    let c = build_client(&fake).await;
    assert!(c.ping().await.is_err());
}

#[tokio::test]
#[serial]
async fn get_artist_returns_api_error_on_failed_response() {
    // `status != "ok"` -> `==` skips the Api return and fails later as a Parse
    // error, so only matching the Api kind + code distinguishes the two.
    let fake = FakeSubsonic::start().await;
    fake.expect_error("getArtist", 70, "not found").await;
    let c = build_client(&fake).await;
    match c.get_artist("a1").await {
        Err(SubsonicError::Api { code, .. }) => assert_eq!(code, 70),
        other => panic!("expected Api error 70, got {other:?}"),
    }
}

#[tokio::test]
#[serial]
async fn get_album_returns_api_error_on_failed_response() {
    let fake = FakeSubsonic::start().await;
    fake.expect_error("getAlbum", 70, "not found").await;
    let c = build_client(&fake).await;
    match c.get_album("al1").await {
        Err(SubsonicError::Api { code, .. }) => assert_eq!(code, 70),
        other => panic!("expected Api error 70, got {other:?}"),
    }
}

#[tokio::test]
#[serial]
async fn get_artist_errors_on_failed_status_without_error_object() {
    let fake = FakeSubsonic::start().await;
    fake.expect_failed_without_error("getArtist").await;
    let c = build_client(&fake).await;
    match c.get_artist("a1").await {
        Err(SubsonicError::Api { code, .. }) => assert_eq!(code, 0),
        other => panic!("non-ok without an error object must be an Api error, got {other:?}"),
    }
}

#[tokio::test]
#[serial]
async fn get_album_errors_on_failed_status_without_error_object() {
    let fake = FakeSubsonic::start().await;
    fake.expect_failed_without_error("getAlbum").await;
    let c = build_client(&fake).await;
    match c.get_album("al1").await {
        Err(SubsonicError::Api { code, .. }) => assert_eq!(code, 0),
        other => panic!("non-ok without an error object must be an Api error, got {other:?}"),
    }
}

#[tokio::test]
#[serial]
async fn get_playlist_errors_on_failed_status_without_error_object() {
    let fake = FakeSubsonic::start().await;
    fake.expect_failed_without_error("getPlaylist").await;
    let c = build_client(&fake).await;
    match c.get_playlist("p1").await {
        Err(SubsonicError::Api { code, .. }) => assert_eq!(code, 0),
        other => panic!("non-ok without an error object must be an Api error, got {other:?}"),
    }
}

#[tokio::test]
#[serial]
async fn get_open_subsonic_extensions_lists_names() {
    let fake = FakeSubsonic::start().await;
    fake.expect_open_subsonic_extensions(&["playbackReport", "transcoding"])
        .await;
    let c = build_client(&fake).await;
    let exts = c.get_open_subsonic_extensions().await.unwrap();
    assert!(
        exts.iter().any(|e| e == "playbackReport"),
        "extension names parsed; got {exts:?}"
    );
}

#[tokio::test]
#[serial]
async fn scrobble_sends_id_and_submission() {
    let fake = FakeSubsonic::start().await;
    fake.expect_scrobble().await;
    let c = build_client(&fake).await;
    c.scrobble("trk-7", true, None).await.unwrap();
    let reqs = fake.received_requests().await;
    let r = reqs
        .iter()
        .find(|r| r.url.path() == "/rest/scrobble")
        .expect("scrobble request sent");
    let q = r.url.query().unwrap_or_default();
    assert!(
        q.contains("id=trk-7") && q.contains("submission=true"),
        "id + submission in query; was {q}"
    );
}

#[tokio::test]
#[serial]
async fn report_playback_sends_state_position_and_media_type() {
    let fake = FakeSubsonic::start().await;
    fake.expect_report_playback().await;
    let c = build_client(&fake).await;
    c.report_playback("m-1", 1234, "stopped", false)
        .await
        .unwrap();
    let reqs = fake.received_requests().await;
    let r = reqs
        .iter()
        .find(|r| r.url.path() == "/rest/reportPlayback")
        .expect("reportPlayback request sent");
    let q = r.url.query().unwrap_or_default();
    assert!(
        q.contains("mediaId=m-1")
            && q.contains("mediaType=song")
            && q.contains("positionMs=1234")
            && q.contains("state=stopped"),
        "reportPlayback params in query; was {q}"
    );
}

#[tokio::test]
#[serial]
async fn get_playlist_returns_api_error_on_failed_response() {
    let fake = FakeSubsonic::start().await;
    fake.expect_error("getPlaylist", 70, "not found").await;
    let c = build_client(&fake).await;
    match c.get_playlist("p1").await {
        Err(SubsonicError::Api { code, .. }) => assert_eq!(code, 70),
        other => panic!("expected Api error 70, got {other:?}"),
    }
}

#[tokio::test]
#[serial]
async fn get_artists_parses_empty_list() {
    let fake = FakeSubsonic::start().await;
    fake.expect_artists(&[]).await;
    let c = build_client(&fake).await;
    let r = c.get_artists().await.unwrap();
    assert!(r.is_empty());
}

#[tokio::test]
#[serial]
async fn get_artists_parses_multi_letter_indexes() {
    let fake = FakeSubsonic::start().await;
    fake.expect_artists(&["Abba", "Beach Boys", "Cure"]).await;
    let c = build_client(&fake).await;
    let r = c.get_artists().await.unwrap();
    assert_eq!(r.len(), 3);
}

#[tokio::test]
#[serial]
async fn get_artist_parses_albums() {
    let fake = FakeSubsonic::start().await;
    fake.expect_get_artist("a0", "Pixies", &["Doolittle", "Surfer Rosa"])
        .await;
    let c = build_client(&fake).await;
    let (artist, albums) = c.get_artist("a0").await.unwrap();
    assert_eq!(artist.name, "Pixies");
    assert_eq!(albums.len(), 2);
}

#[tokio::test]
#[serial]
async fn get_artist_returns_error_on_unknown_id() {
    let fake = FakeSubsonic::start().await;
    fake.expect_error("getArtist", 70, "not found").await;
    let c = build_client(&fake).await;
    assert!(c.get_artist("nope").await.is_err());
}

#[tokio::test]
#[serial]
async fn get_album_parses_songs() {
    let fake = FakeSubsonic::start().await;
    fake.expect_get_album("alb0", "Disintegration", &["Lullaby", "Pictures of You"])
        .await;
    let c = build_client(&fake).await;
    let (album, songs) = c.get_album("alb0").await.unwrap();
    assert_eq!(album.name, "Disintegration");
    assert_eq!(songs.len(), 2);
}

#[tokio::test]
#[serial]
async fn get_album_propagates_http_500() {
    let fake = FakeSubsonic::start().await;
    fake.expect_http_status("getAlbum", 500).await;
    let c = build_client(&fake).await;
    assert!(c.get_album("any").await.is_err());
}

#[tokio::test]
#[serial]
async fn get_playlists_with_data() {
    let fake = FakeSubsonic::start().await;
    fake.expect_get_playlists_with(&[("p0", "Liked"), ("p1", "Workout")])
        .await;
    let c = build_client(&fake).await;
    let pls = c.get_playlists().await.unwrap();
    assert_eq!(pls.len(), 2);
    assert!(pls.iter().any(|p| p.name == "Liked"));
}

#[tokio::test]
#[serial]
async fn get_playlist_parses_songs() {
    let fake = FakeSubsonic::start().await;
    fake.expect_get_playlist("p0", "Mix", &["A", "B", "C"])
        .await;
    let c = build_client(&fake).await;
    let (pl, songs) = c.get_playlist("p0").await.unwrap();
    assert_eq!(pl.name, "Mix");
    assert_eq!(songs.len(), 3);
}

#[tokio::test]
#[serial]
async fn get_starred_songs_returns_list() {
    let fake = FakeSubsonic::start().await;
    fake.expect_starred_with(&["Star1", "Star2"]).await;
    let c = build_client(&fake).await;
    let songs = c.get_starred_songs().await.unwrap();
    assert_eq!(songs.len(), 2);
}

#[tokio::test]
#[serial]
async fn get_random_songs_returns_list() {
    let fake = FakeSubsonic::start().await;
    fake.expect_random_songs(&["R1", "R2", "R3"]).await;
    let c = build_client(&fake).await;
    let songs = c.get_random_songs().await.unwrap();
    assert_eq!(songs.len(), 3);
}

#[tokio::test]
#[serial]
async fn star_song_succeeds() {
    let fake = FakeSubsonic::start().await;
    fake.expect_star().await;
    let c = build_client(&fake).await;
    c.star_song("s0").await.expect("star ok");
}

#[tokio::test]
#[serial]
async fn unstar_song_succeeds() {
    let fake = FakeSubsonic::start().await;
    fake.expect_unstar().await;
    let c = build_client(&fake).await;
    c.unstar_song("s0").await.expect("unstar ok");
}

#[tokio::test]
#[serial]
async fn get_cover_art_returns_raw_bytes() {
    let fake = FakeSubsonic::start().await;
    let bytes = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
    fake.expect_get_cover_art("art-1", bytes.clone()).await;
    let c = build_client(&fake).await;
    let got = c.get_cover_art("art-1", 256).await.unwrap();
    assert_eq!(got, bytes);
}

#[tokio::test]
#[serial]
async fn stream_url_includes_song_id_and_auth() {
    let fake = FakeSubsonic::start().await;
    let c = build_client(&fake).await;
    let url = c.get_stream_url("song-42").unwrap();
    assert!(
        url.contains("song-42"),
        "stream url must include song id; got {}",
        url
    );
    assert!(url.contains("u=test"), "auth params must be present");
}

#[tokio::test]
#[serial]
async fn stream_url_for_url_encoded_song_id() {
    let fake = FakeSubsonic::start().await;
    let c = build_client(&fake).await;
    let url = c.get_stream_url("id with spaces").unwrap();
    assert!(
        url.contains("id%20with%20spaces") || url.contains("id+with+spaces"),
        "spaces must be encoded; got {}",
        url
    );
}

#[tokio::test]
#[serial]
async fn search3_passes_query_and_counts_correctly() {
    let fake = FakeSubsonic::start().await;
    fake.expect_search3(&["A"], &["B"], &["C"]).await;
    let c = build_client(&fake).await;
    let r = c.search3("hello", 5, 5, 5).await.unwrap();
    assert_eq!(r.artist.len(), 1);
    assert_eq!(r.album.len(), 1);
    assert_eq!(r.song.len(), 1);
}

#[tokio::test]
#[serial]
async fn empty_search_returns_empty_result() {
    let fake = FakeSubsonic::start().await;
    fake.expect_search3(&[], &[], &[]).await;
    let c = build_client(&fake).await;
    let r = c.search3("nothing", 5, 5, 5).await.unwrap();
    assert!(r.artist.is_empty() && r.album.is_empty() && r.song.is_empty());
}

#[tokio::test]
#[serial]
async fn invalid_base_url_returns_construction_error() {
    let r = SubsonicClient::new("not a url", "u", &"p".into());
    assert!(r.is_err());
}
