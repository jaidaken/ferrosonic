//! `DaemonCore::search` against the fake Subsonic `search3` endpoint.

mod common;

use common::TestDaemon;
use serial_test::serial;

#[tokio::test]
#[serial]
async fn search_parses_artists_albums_songs() {
    let td = TestDaemon::new().await;
    td.fake_subsonic
        .expect_search3(&["The Cure"], &["Disintegration"], &["Lullaby"])
        .await;

    let results = td.core.search("cure", 5, 5, 5).await;

    assert_eq!(results.artist.len(), 1);
    assert_eq!(results.artist[0].name, "The Cure");
    assert_eq!(results.album.len(), 1);
    assert_eq!(results.album[0].name, "Disintegration");
    assert_eq!(results.song.len(), 1);
    assert_eq!(results.song[0].title, "Lullaby");
}

#[tokio::test]
#[serial]
async fn search_handles_empty_response() {
    let td = TestDaemon::new().await;
    td.fake_subsonic.expect_search3(&[], &[], &[]).await;

    let results = td.core.search("nothing", 5, 5, 5).await;

    assert!(results.artist.is_empty());
    assert!(results.album.is_empty());
    assert!(results.song.is_empty());
}

#[tokio::test]
#[serial]
async fn search_passes_query_and_counts_in_url() {
    let td = TestDaemon::new().await;
    td.fake_subsonic
        .expect_search3(&["X"], &["Y"], &["Z"])
        .await;

    td.core.search("my query", 3, 7, 11).await;

    let reqs = td.fake_subsonic.received_requests().await;
    assert!(
        !reqs.is_empty(),
        "fake subsonic must have received the search"
    );
    let req = reqs
        .iter()
        .find(|r| r.url.path() == "/rest/search3")
        .unwrap();
    let query = req.url.query().unwrap_or_default();
    assert!(
        query.contains("query=my%20query"),
        "query string must url-encode: got {}",
        query
    );
    assert!(
        query.contains("artistCount=3"),
        "expected artistCount=3 in {}",
        query
    );
    assert!(
        query.contains("albumCount=7"),
        "expected albumCount=7 in {}",
        query
    );
    assert!(
        query.contains("songCount=11"),
        "expected songCount=11 in {}",
        query
    );
}

#[tokio::test]
#[serial]
async fn search_with_no_subsonic_client_returns_empty() {
    let td = TestDaemon::new().await;
    {
        let mut sub = td.core.subsonic.write().await;
        *sub = None;
    }

    let results = td.core.search("anything", 5, 5, 5).await;
    assert!(results.artist.is_empty());
    assert!(results.album.is_empty());
    assert!(results.song.is_empty());
}
