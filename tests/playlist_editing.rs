//! Playlist editing: rename / delete / add / remove / reorder map to the
//! correct Subsonic endpoints, and the Playlists-page keys drive them.

mod common;

use common::TestDaemon;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ferrosonic::app::App;
use ferrosonic::subsonic::models::{Child, Playlist};
use serial_test::serial;
use wiremock::Request;

fn key(code: KeyCode) -> KeyEvent {
    let mut k = KeyEvent::new(code, KeyModifiers::NONE);
    k.kind = KeyEventKind::Press;
    k
}

fn find<'a>(reqs: &'a [Request], path: &str) -> &'a Request {
    match reqs.iter().find(|r| r.url.path() == path) {
        Some(r) => r,
        None => {
            let seen: Vec<_> = reqs.iter().map(|r| r.url.path().to_string()).collect();
            panic!("no request to {path}; saw {seen:?}");
        }
    }
}

#[tokio::test]
#[serial]
async fn rename_sends_updateplaylist_with_id_and_name() {
    let td = TestDaemon::new().await;
    td.fake_subsonic.expect_update_playlist().await;
    td.fake_subsonic.expect_playlists().await;

    td.core.rename_playlist("pl-7", "Road Trip").await.unwrap();

    let reqs = td.fake_subsonic.received_requests().await;
    let q = find(&reqs, "/rest/updatePlaylist")
        .url
        .query()
        .unwrap_or_default();
    assert!(q.contains("playlistId=pl-7"), "query was {q}");
    assert!(
        q.contains("name=Road%20Trip") || q.contains("name=Road+Trip"),
        "the new name is url-encoded; query was {q}"
    );
}

#[tokio::test]
#[serial]
async fn delete_sends_deleteplaylist_with_id() {
    let td = TestDaemon::new().await;
    td.fake_subsonic.expect_delete_playlist().await;
    td.fake_subsonic.expect_playlists().await;

    td.core.delete_playlist("pl-3").await.unwrap();

    let reqs = td.fake_subsonic.received_requests().await;
    let q = find(&reqs, "/rest/deletePlaylist")
        .url
        .query()
        .unwrap_or_default();
    assert!(q.contains("id=pl-3"), "query was {q}");
}

#[tokio::test]
#[serial]
async fn add_song_sends_updateplaylist_with_songidtoadd() {
    let td = TestDaemon::new().await;
    td.fake_subsonic.expect_update_playlist().await;
    td.fake_subsonic
        .expect_get_playlist("pl-1", "Mix", &["a", "b"])
        .await;
    td.fake_subsonic.expect_playlists().await;

    let songs = td.core.playlist_add_song("pl-1", "song-9").await.unwrap();

    let reqs = td.fake_subsonic.received_requests().await;
    let q = find(&reqs, "/rest/updatePlaylist")
        .url
        .query()
        .unwrap_or_default();
    assert!(q.contains("playlistId=pl-1"), "query was {q}");
    assert!(q.contains("songIdToAdd=song-9"), "query was {q}");
    assert_eq!(songs.len(), 2, "returns the refreshed playlist songs");
}

#[tokio::test]
#[serial]
async fn remove_song_sends_updateplaylist_with_songindextoremove() {
    let td = TestDaemon::new().await;
    td.fake_subsonic.expect_update_playlist().await;
    td.fake_subsonic
        .expect_get_playlist("pl-1", "Mix", &["a"])
        .await;
    td.fake_subsonic.expect_playlists().await;

    let songs = td.core.playlist_remove_song("pl-1", 2).await.unwrap();

    let reqs = td.fake_subsonic.received_requests().await;
    let q = find(&reqs, "/rest/updatePlaylist")
        .url
        .query()
        .unwrap_or_default();
    assert!(q.contains("playlistId=pl-1"), "query was {q}");
    assert!(q.contains("songIndexToRemove=2"), "query was {q}");
    assert_eq!(songs.len(), 1, "returns the refreshed playlist songs");
}

#[tokio::test]
#[serial]
async fn reorder_sends_createplaylist_with_ordered_song_ids() {
    let td = TestDaemon::new().await;
    td.fake_subsonic.expect_create_playlist().await;
    td.fake_subsonic
        .expect_get_playlist("pl-1", "Mix", &["x", "y", "z"])
        .await;
    td.fake_subsonic.expect_playlists().await;

    let order = vec!["z".to_string(), "x".to_string(), "y".to_string()];
    td.core.playlist_reorder("pl-1", &order).await.unwrap();

    let reqs = td.fake_subsonic.received_requests().await;
    let q = find(&reqs, "/rest/createPlaylist")
        .url
        .query()
        .unwrap_or_default();
    assert!(q.contains("playlistId=pl-1"), "query was {q}");
    let z = q.find("songId=z").expect("z present");
    let x = q.find("songId=x").expect("x present");
    let y = q.find("songId=y").expect("y present");
    assert!(
        z < x && x < y,
        "song ids are sent in the requested order; query was {q}"
    );
}

async fn playlists_app(playlists: &[(&str, &str)], songs: &[&str]) -> (App, TestDaemon) {
    let td = TestDaemon::new().await;
    let cfg = td.state.read().await.config.clone();
    let mut app = App::with_remote_client(
        std::sync::Arc::new(ferrosonic::ipc::InProcessClient::new(td.core.clone())),
        cfg,
    );
    {
        let mut ds = app.daemon_state.write().await;
        ds.library.playlists = playlists
            .iter()
            .map(|(id, name)| Playlist {
                id: (*id).into(),
                name: (*name).into(),
                owner: None,
                song_count: Some(songs.len() as i32),
                duration: None,
                cover_art: None,
                public: None,
                comment: None,
            })
            .collect();
    }
    {
        let mut cs = app.client_state.write().await;
        cs.playlists.selected_playlist = (!playlists.is_empty()).then_some(0);
        cs.playlists.songs = songs
            .iter()
            .map(|t| Child {
                id: (*t).into(),
                title: (*t).into(),
                ..Default::default()
            })
            .collect();
        cs.playlists.selected_song = (!songs.is_empty()).then_some(0);
    }
    app.handle_key(key(KeyCode::F(4))).await.unwrap();
    (app, td)
}

#[tokio::test]
#[serial]
async fn shift_r_opens_the_rename_box_seeded_with_the_name() {
    let (mut app, _td) = playlists_app(&[("pl-1", "Old Name")], &[]).await;
    app.handle_key(key(KeyCode::Char('R'))).await.unwrap();
    let cs = app.client_state.read().await;
    assert!(cs.playlists.renaming, "R opens the rename box");
    assert_eq!(
        cs.playlists.rename_buf, "Old Name",
        "the box is seeded with the current name for editing"
    );
}

#[tokio::test]
#[serial]
async fn shift_d_then_y_sends_deleteplaylist() {
    let (mut app, td) = playlists_app(&[("pl-9", "Doomed")], &[]).await;
    td.fake_subsonic.expect_delete_playlist().await;
    td.fake_subsonic.expect_playlists().await;

    app.handle_key(key(KeyCode::Char('D'))).await.unwrap();
    {
        let cs = app.client_state.read().await;
        assert!(cs.playlists.confirming_delete, "D opens the confirm prompt");
    }
    app.handle_key(key(KeyCode::Char('y'))).await.unwrap();

    let reqs = td.fake_subsonic.received_requests().await;
    let q = find(&reqs, "/rest/deletePlaylist")
        .url
        .query()
        .unwrap_or_default();
    assert!(q.contains("id=pl-9"), "query was {q}");
}

#[tokio::test]
#[serial]
async fn a_opens_picker_and_enter_adds_song_to_selected_playlist() {
    let (mut app, td) = playlists_app(&[("pl-a", "Target")], &["song-x"]).await;
    td.fake_subsonic.expect_update_playlist().await;
    td.fake_subsonic
        .expect_get_playlist("pl-a", "Target", &["song-x"])
        .await;
    td.fake_subsonic.expect_playlists().await;

    {
        let mut cs = app.client_state.write().await;
        cs.playlists.focus = 1;
    }
    app.handle_key(key(KeyCode::Char('a'))).await.unwrap();
    {
        let cs = app.client_state.read().await;
        assert!(
            cs.playlist_picker.active,
            "a opens the add-to-playlist picker"
        );
    }
    app.handle_key(key(KeyCode::Enter)).await.unwrap();
    {
        let cs = app.client_state.read().await;
        assert!(!cs.playlist_picker.active, "Enter closes the picker");
    }

    let reqs = td.fake_subsonic.received_requests().await;
    let q = find(&reqs, "/rest/updatePlaylist")
        .url
        .query()
        .unwrap_or_default();
    assert!(q.contains("playlistId=pl-a"), "query was {q}");
    assert!(q.contains("songIdToAdd=song-x"), "query was {q}");
}

#[tokio::test]
#[serial]
async fn d_removes_song_and_reconciles_pane_from_the_server_list() {
    let (mut app, td) = playlists_app(&[("pl-1", "Mix")], &["s0", "s1", "s2"]).await;
    td.fake_subsonic.expect_update_playlist().await;
    // Server's authoritative post-edit list differs from the optimistic guess
    // (3 - 1 = 2), so a len of 1 proves the pane reconciled to the response.
    td.fake_subsonic
        .expect_get_playlist("pl-1", "Mix", &["only"])
        .await;
    td.fake_subsonic.expect_playlists().await;

    {
        let mut cs = app.client_state.write().await;
        cs.playlists.focus = 1;
        cs.playlists.selected_song = Some(0);
    }
    app.handle_key(key(KeyCode::Char('d'))).await.unwrap();

    let cs = app.client_state.read().await;
    assert_eq!(
        cs.playlists.songs.len(),
        1,
        "the pane reflects the server's authoritative list, not the optimistic 2"
    );
    assert_eq!(
        cs.playlists.songs[0].id, "song-0",
        "reconciled to the server response entry"
    );
}
