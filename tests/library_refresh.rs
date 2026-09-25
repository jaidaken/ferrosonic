//! Library changes on the server reach the daemon caches and the TUI mirror
//! without a daemon restart: explicit refresh, server scans, and in-flight fetches.

mod common;

use std::sync::Arc;

use common::{render::empty_cover_art_state, TestDaemon};
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ferrosonic::app::page_state::LibraryView;
use ferrosonic::app::{apply_event, App};
use ferrosonic::daemon::library_watch::{ScanPoll, ScanWatch};
use ferrosonic::ipc::client::DaemonClient;
use ferrosonic::ipc::{DaemonEvent, DaemonRequest, DaemonResponse, InProcessClient};
use ferrosonic::secret::Secret;
use ferrosonic::subsonic::models::{Album, Child};
use serde_json::json;
use serial_test::serial;

fn titles(songs: &[Child]) -> Vec<String> {
    songs.iter().map(|s| s.title.clone()).collect()
}

fn album_names(albums: &[Album]) -> Vec<String> {
    albums.iter().map(|a| a.name.clone()).collect()
}

fn album(id: &str, name: &str) -> Album {
    serde_json::from_value(json!({ "id": id, "name": name })).expect("album fixture parses")
}

async fn load_album(client: &InProcessClient, id: &str) -> Vec<Child> {
    match client
        .request(DaemonRequest::LoadAlbum(id.to_string()))
        .await
    {
        Ok(DaemonResponse::AlbumSongs(songs)) => songs,
        other => panic!("LoadAlbum returned {other:?}"),
    }
}

/// Server state behind a library refresh; empty folders keep the first-run default out of the way.
async fn mount_refresh_endpoints(td: &TestDaemon) {
    td.fake_subsonic.expect_artists(&["Artist"]).await;
    td.fake_subsonic.expect_music_folders(&[]).await;
    td.fake_subsonic.expect_starred().await;
    td.fake_subsonic.expect_playlists().await;
}

#[tokio::test]
#[serial]
async fn refresh_serves_tracks_added_to_an_already_opened_album() {
    let td = TestDaemon::new().await;
    let client = InProcessClient::new(td.core.clone());
    td.fake_subsonic
        .expect_get_album("al-1", "Album", &["One"])
        .await;
    assert_eq!(titles(&load_album(&client, "al-1").await), ["One"]);

    td.fake_subsonic.reset().await;
    td.fake_subsonic
        .expect_get_album("al-1", "Album", &["One", "Two"])
        .await;
    mount_refresh_endpoints(&td).await;
    client
        .request(DaemonRequest::RefreshArtists)
        .await
        .expect("refresh succeeds");

    assert_eq!(
        titles(&load_album(&client, "al-1").await),
        ["One", "Two"],
        "a refresh must drop the cached track list so the new track shows"
    );
}

#[tokio::test]
#[serial]
async fn ctrl_r_replaces_album_lists_the_tui_already_holds() {
    let td = TestDaemon::new().await;
    mount_refresh_endpoints(&td).await;
    td.fake_subsonic
        .expect_get_artist("ar-1", "Artist", &["Old", "New"])
        .await;
    let client: Arc<dyn DaemonClient> = Arc::new(InProcessClient::new(td.core.clone()));
    let cfg = td.state.read().await.config.clone();
    let mut app = App::with_remote_client(client.clone(), cfg);
    {
        let mut ds = app.daemon_state.write().await;
        let lib = &mut ds.library;
        ferrosonic::daemon::library::cache_insert(
            &mut lib.albums_cache,
            &mut lib.albums_cache_order,
            "ar-1".into(),
            vec![album("alb-0", "Old")],
            ferrosonic::daemon::library::ALBUMS_CACHE_CAP,
        );
    }
    app.client_state
        .write()
        .await
        .artists
        .expanded
        .insert("ar-1".into());
    let cover_art = empty_cover_art_state();
    let mut rx = client.subscribe();

    let mut ctrl_r = KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL);
    ctrl_r.kind = KeyEventKind::Press;
    app.handle_key(ctrl_r).await.expect("ctrl+r handled");
    while let Ok(ev) = rx.try_recv() {
        apply_event(
            &app.daemon_state,
            &app.client_state,
            &client,
            &cover_art,
            ev,
        )
        .await;
    }

    let ds = app.daemon_state.read().await;
    let albums = ds
        .library
        .albums_cache
        .get("ar-1")
        .map(|a| album_names(a))
        .unwrap_or_default();
    assert_eq!(
        albums,
        ["Old", "New"],
        "the expanded artist must show the album the server gained"
    );
}

fn drain(rx: &mut tokio::sync::broadcast::Receiver<DaemonEvent>) -> Vec<DaemonEvent> {
    std::iter::from_fn(|| rx.try_recv().ok()).collect()
}

fn saw_invalidation(events: &[DaemonEvent]) -> bool {
    events
        .iter()
        .any(|e| matches!(e, DaemonEvent::LibraryInvalidated))
}

#[tokio::test]
#[serial]
async fn finished_server_scan_resets_caches_and_refetches_the_library() {
    let td = TestDaemon::new().await;
    let client = InProcessClient::new(td.core.clone());
    td.fake_subsonic
        .expect_get_album("al-1", "Album", &["One"])
        .await;
    td.fake_subsonic
        .expect_scan_status(false, 10, 3, Some("2026-09-25T00:00:00Z"))
        .await;
    mount_refresh_endpoints(&td).await;
    assert_eq!(titles(&load_album(&client, "al-1").await), ["One"]);
    let mut watch = ScanWatch::default();

    assert_eq!(
        td.core.poll_library_scan(&mut watch).await,
        ScanPoll::Baseline
    );
    assert_eq!(td.fake_subsonic.request_count("getArtists").await, 0);

    td.fake_subsonic.reset().await;
    td.fake_subsonic
        .expect_scan_status(false, 12, 3, Some("2026-09-25T00:05:00Z"))
        .await;
    td.fake_subsonic
        .expect_get_album("al-1", "Album", &["One", "Two"])
        .await;
    mount_refresh_endpoints(&td).await;
    let mut rx = td.core.subscribe();

    assert_eq!(
        td.core.poll_library_scan(&mut watch).await,
        ScanPoll::Refreshed
    );
    assert_eq!(td.fake_subsonic.request_count("getArtists").await, 1);
    assert!(saw_invalidation(&drain(&mut rx)));
    assert_eq!(titles(&load_album(&client, "al-1").await), ["One", "Two"]);
}

#[tokio::test]
#[serial]
async fn running_scan_defers_the_refresh_until_it_finishes() {
    let td = TestDaemon::new().await;
    td.fake_subsonic
        .expect_scan_status(false, 10, 3, Some("t1"))
        .await;
    mount_refresh_endpoints(&td).await;
    let mut watch = ScanWatch::default();
    assert_eq!(
        td.core.poll_library_scan(&mut watch).await,
        ScanPoll::Baseline
    );

    td.fake_subsonic.reset().await;
    td.fake_subsonic
        .expect_scan_status(true, 4, 1, Some("t1"))
        .await;
    mount_refresh_endpoints(&td).await;
    assert_eq!(
        td.core.poll_library_scan(&mut watch).await,
        ScanPoll::Scanning
    );
    assert_eq!(td.fake_subsonic.request_count("getArtists").await, 0);

    td.fake_subsonic.reset().await;
    td.fake_subsonic
        .expect_scan_status(false, 11, 3, Some("t2"))
        .await;
    mount_refresh_endpoints(&td).await;
    assert_eq!(
        td.core.poll_library_scan(&mut watch).await,
        ScanPoll::Refreshed
    );
    assert_eq!(td.fake_subsonic.request_count("getArtists").await, 1);
}

#[tokio::test]
#[serial]
async fn unchanged_scan_status_leaves_the_caches_alone() {
    let td = TestDaemon::new().await;
    let client = InProcessClient::new(td.core.clone());
    td.fake_subsonic
        .expect_scan_status(false, 10, 3, Some("t1"))
        .await;
    td.fake_subsonic
        .expect_get_album("al-1", "Album", &["One"])
        .await;
    load_album(&client, "al-1").await;
    let mut watch = ScanWatch::default();

    assert_eq!(
        td.core.poll_library_scan(&mut watch).await,
        ScanPoll::Baseline
    );
    assert_eq!(
        td.core.poll_library_scan(&mut watch).await,
        ScanPoll::Unchanged
    );

    assert!(td
        .state
        .read()
        .await
        .library
        .album_songs_cache
        .contains_key("al-1"));
    assert_eq!(td.fake_subsonic.request_count("getArtists").await, 0);
}

#[tokio::test]
#[serial]
async fn album_fetched_across_a_reset_is_returned_but_not_cached() {
    let td = TestDaemon::new().await;
    td.fake_subsonic
        .expect_get_album_with_delay("al-1", "Album", &["Old"], 300)
        .await;
    let core = td.core.clone();
    let fetch = tokio::spawn(async move { core.load_album_songs("al-1").await });
    for _ in 0..200 {
        if td.fake_subsonic.request_count("getAlbum").await > 0 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    td.core.invalidate_library_caches().await;

    let songs = tokio::time::timeout(std::time::Duration::from_secs(10), fetch)
        .await
        .expect("fetch finishes")
        .expect("fetch task joins");

    assert_eq!(titles(&songs), ["Old"], "the caller still gets its reply");
    assert!(
        !td.state
            .read()
            .await
            .library
            .album_songs_cache
            .contains_key("al-1"),
        "a reply from before the reset must not refill the cache"
    );
}

#[tokio::test]
#[serial]
async fn server_switch_drops_the_previous_servers_caches() {
    let td = TestDaemon::new().await;
    let client = InProcessClient::new(td.core.clone());
    td.fake_subsonic
        .expect_get_album("al-1", "Album", &["One"])
        .await;
    mount_refresh_endpoints(&td).await;
    load_album(&client, "al-1").await;
    let mut rx = td.core.subscribe();

    td.core
        .update_server_config(
            &td.fake_subsonic.url(),
            "test",
            &Secret::from_string("test".to_string()),
        )
        .await
        .expect("server config saves");

    assert!(td.state.read().await.library.album_songs_cache.is_empty());
    assert!(saw_invalidation(&drain(&mut rx)));
}

#[tokio::test]
#[serial]
async fn folder_switch_drops_the_previous_folders_caches() {
    let td = TestDaemon::new().await;
    let client = InProcessClient::new(td.core.clone());
    td.fake_subsonic
        .expect_get_album("al-1", "Album", &["One"])
        .await;
    td.fake_subsonic.expect_artists(&["A"]).await;
    td.fake_subsonic.expect_random_songs(&["s"]).await;
    load_album(&client, "al-1").await;
    let mut rx = td.core.subscribe();

    td.core.set_music_folder(Some(2)).await.expect("folder set");

    assert!(td.state.read().await.library.album_songs_cache.is_empty());
    assert!(saw_invalidation(&drain(&mut rx)));
}

#[tokio::test]
#[serial]
async fn open_album_list_reloads_and_keeps_the_selected_album() {
    let td = TestDaemon::new().await;
    td.fake_subsonic
        .expect_album_list2(&[("a1", "Alpha"), ("a2", "Beta"), ("a3", "Gamma")])
        .await;
    let client: Arc<dyn DaemonClient> = Arc::new(InProcessClient::new(td.core.clone()));
    let cfg = td.state.read().await.config.clone();
    let app = App::with_remote_client(client.clone(), cfg);
    {
        let mut cs = app.client_state.write().await;
        cs.artists.view = LibraryView::AlbumList;
        cs.artists.albums = vec![album("a1", "Alpha"), album("a3", "Gamma")];
        cs.artists.album_selected = Some(1);
    }

    apply_event(
        &app.daemon_state,
        &app.client_state,
        &client,
        &empty_cover_art_state(),
        DaemonEvent::LibraryInvalidated,
    )
    .await;

    let cs = app.client_state.read().await;
    assert_eq!(album_names(&cs.artists.albums), ["Alpha", "Beta", "Gamma"]);
    assert_eq!(cs.artists.album_selected, Some(2), "Gamma stays selected");
}
