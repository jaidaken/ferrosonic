//! Edge cases of the library reset: failed refreshes, failed fetches, scans in
//! progress, cover art in flight, and user input during a reload.

mod common;

use std::sync::Arc;

use common::{render::empty_cover_art_state, TestDaemon};
use ferrosonic::app::page_state::{AlbumSort, LibraryView};
use ferrosonic::app::{apply_event, App};
use ferrosonic::daemon::library_watch::{ScanPoll, ScanWatch};
use ferrosonic::ipc::client::DaemonClient;
use ferrosonic::ipc::{DaemonEvent, InProcessClient};
use serial_test::serial;

async fn mount_refresh_endpoints(td: &TestDaemon) {
    td.fake_subsonic.expect_artists(&["Artist"]).await;
    td.fake_subsonic.expect_music_folders(&[]).await;
    td.fake_subsonic.expect_starred().await;
    td.fake_subsonic.expect_playlists().await;
}

#[tokio::test]
#[serial]
async fn failed_refresh_after_a_scan_is_retried_on_the_next_poll() {
    let td = TestDaemon::new().await;
    td.fake_subsonic
        .expect_scan_status(false, 10, 3, Some("t1"))
        .await;
    let mut watch = ScanWatch::default();
    assert_eq!(
        td.core.poll_library_scan(&mut watch).await,
        ScanPoll::Baseline
    );

    td.fake_subsonic.reset().await;
    td.fake_subsonic
        .expect_scan_status(false, 12, 3, Some("t2"))
        .await;
    td.fake_subsonic.expect_http_status("getArtists", 500).await;
    td.fake_subsonic.expect_music_folders(&[]).await;
    td.fake_subsonic.expect_starred().await;
    td.fake_subsonic.expect_playlists().await;
    assert_ne!(
        td.core.poll_library_scan(&mut watch).await,
        ScanPoll::Refreshed,
        "a refresh whose artist fetch failed is not a completed refresh"
    );

    td.fake_subsonic.reset().await;
    td.fake_subsonic
        .expect_scan_status(false, 12, 3, Some("t2"))
        .await;
    mount_refresh_endpoints(&td).await;
    assert_eq!(
        td.core.poll_library_scan(&mut watch).await,
        ScanPoll::Refreshed,
        "the same finished scan is refreshed again once the server answers"
    );
    assert_eq!(td.fake_subsonic.request_count("getArtists").await, 1);
}

#[tokio::test]
#[serial]
async fn scan_running_when_the_watcher_starts_refreshes_when_it_ends() {
    let td = TestDaemon::new().await;
    td.fake_subsonic
        .expect_scan_status(true, 4, 1, Some("t1"))
        .await;
    let mut watch = ScanWatch::default();
    assert_eq!(
        td.core.poll_library_scan(&mut watch).await,
        ScanPoll::Scanning
    );

    td.fake_subsonic.reset().await;
    td.fake_subsonic
        .expect_scan_status(false, 12, 3, Some("t2"))
        .await;
    mount_refresh_endpoints(&td).await;
    assert_eq!(
        td.core.poll_library_scan(&mut watch).await,
        ScanPoll::Refreshed,
        "the scan seen at startup may have added music the startup fetch missed"
    );
    assert_eq!(td.fake_subsonic.request_count("getArtists").await, 1);
}

#[tokio::test]
#[serial]
async fn failed_artist_fetch_during_a_reload_is_not_cached_as_empty() {
    let td = TestDaemon::new().await;
    td.fake_subsonic.expect_http_status("getArtist", 500).await;
    let client: Arc<dyn DaemonClient> = Arc::new(InProcessClient::new(td.core.clone()));
    let cfg = td.state.read().await.config.clone();
    let app = App::with_remote_client(client.clone(), cfg);
    app.client_state
        .write()
        .await
        .artists
        .expanded
        .insert("ar-1".into());

    apply_event(
        &app.daemon_state,
        &app.client_state,
        &client,
        &empty_cover_art_state(),
        DaemonEvent::LibraryInvalidated,
    )
    .await;

    assert!(
        !app.daemon_state
            .read()
            .await
            .library
            .albums_cache
            .contains_key("ar-1"),
        "a failed fetch must leave the artist uncached so the next expand retries"
    );
}

#[tokio::test]
#[serial]
async fn sort_changed_during_an_album_list_reload_is_applied_to_the_new_list() {
    let td = TestDaemon::new().await;
    td.fake_subsonic
        .expect_album_list2_slow(&[("a1", "Alpha", 2001), ("a2", "Beta", 1999)], 400)
        .await;
    let client: Arc<dyn DaemonClient> = Arc::new(InProcessClient::new(td.core.clone()));
    let cfg = td.state.read().await.config.clone();
    let app = App::with_remote_client(client.clone(), cfg);
    {
        let mut cs = app.client_state.write().await;
        cs.artists.view = LibraryView::AlbumList;
        cs.artists.album_sort = AlbumSort::Name;
    }
    let (ds, cs, c) = (
        app.daemon_state.clone(),
        app.client_state.clone(),
        client.clone(),
    );
    let reload = tokio::spawn(async move {
        apply_event(
            &ds,
            &cs,
            &c,
            &empty_cover_art_state(),
            DaemonEvent::LibraryInvalidated,
        )
        .await;
    });
    for _ in 0..200 {
        if td.fake_subsonic.request_count("getAlbumList2").await > 0 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    app.client_state.write().await.artists.album_sort = AlbumSort::ReleaseDate;
    tokio::time::timeout(std::time::Duration::from_secs(10), reload)
        .await
        .expect("reload finishes")
        .expect("reload task joins");

    let cs = app.client_state.read().await;
    let names: Vec<&str> = cs.artists.albums.iter().map(|a| a.name.as_str()).collect();
    assert_eq!(
        names,
        ["Beta", "Alpha"],
        "sorted by release date, oldest first"
    );
}

#[tokio::test]
#[serial]
async fn cover_fetched_across_a_reset_is_not_served_from_the_cache() {
    let td = TestDaemon::new().await;
    td.fake_subsonic
        .expect_get_cover_art_slow("al-9", vec![1, 2, 3], 300)
        .await;
    let core = td.core.clone();
    let fetch = tokio::spawn(async move { core.get_cover_art("al-9", 512).await });
    for _ in 0..200 {
        if td.fake_subsonic.request_count("getCoverArt").await > 0 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    td.core.invalidate_library_caches().await;
    let first = tokio::time::timeout(std::time::Duration::from_secs(10), fetch)
        .await
        .expect("fetch finishes")
        .expect("fetch task joins");
    assert_eq!(first, [1, 2, 3], "the caller still gets its reply");

    td.core.get_cover_art("al-9", 512).await;
    assert_eq!(
        td.fake_subsonic.request_count("getCoverArt").await,
        2,
        "art fetched before the reset must be fetched again, not served from the cache"
    );
}
