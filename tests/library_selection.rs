//! Library (music folder) selection: getMusicFolders populates state, and the
//! selected folder scopes browse calls via musicFolderId.

mod common;

use common::TestDaemon;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ferrosonic::app::App;
use ferrosonic::ipc::protocol::QuickPlayAlbumKind;
use ferrosonic::subsonic::models::{Artist, MusicFolder};
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
async fn refresh_music_folders_populates_state() {
    let td = TestDaemon::new().await;
    td.fake_subsonic
        .expect_music_folders(&[(1, "Music"), (2, "Test")])
        .await;

    td.core.refresh_music_folders().await;

    let st = td.state.read().await;
    assert_eq!(st.library.music_folders.len(), 2, "both folders are stored");
    assert_eq!(st.library.music_folders[1].id, 2);
    assert_eq!(st.library.music_folders[1].name, "Test");
}

#[tokio::test]
#[serial]
async fn set_music_folder_persists_and_scopes_browse_calls() {
    let td = TestDaemon::new().await;
    td.fake_subsonic.expect_artists(&["A"]).await;
    td.fake_subsonic.expect_random_songs(&["s"]).await;

    td.core.set_music_folder(Some(2)).await.unwrap();

    {
        let st = td.state.read().await;
        assert_eq!(
            st.config.music_folder_id,
            Some(2),
            "the selection is persisted to config"
        );
    }

    let reqs = td.fake_subsonic.received_requests().await;
    let q = find(&reqs, "/rest/getArtists")
        .url
        .query()
        .unwrap_or_default();
    assert!(
        q.contains("musicFolderId=2"),
        "the refetch scopes to the selected folder; query was {q}"
    );
}

#[tokio::test]
#[serial]
async fn get_all_albums_pages_past_a_short_first_page() {
    use ferrosonic::ipc::client::DaemonClient;

    let td = TestDaemon::new().await;
    // Server caps at 2 albums per page despite the requested size=500; paging
    // must advance by what it received, not declare the first short page last.
    td.fake_subsonic
        .expect_album_list_page("alphabeticalByName", 0, &["a", "b"])
        .await;
    td.fake_subsonic
        .expect_album_list_page("alphabeticalByName", 2, &["c", "d"])
        .await;
    td.fake_subsonic
        .expect_album_list_page("alphabeticalByName", 4, &[])
        .await;

    let client = ferrosonic::ipc::InProcessClient::new(td.core.clone());
    client
        .request(ferrosonic::ipc::protocol::DaemonRequest::LoadAllAlbums)
        .await
        .expect("load all albums");

    let st = td.state.read().await;
    assert_eq!(
        st.library.all_albums.len(),
        4,
        "both short pages must be fetched before the empty page ends paging"
    );
}

#[tokio::test]
#[serial]
async fn starred_refresh_scopes_to_the_selected_folder() {
    let td = TestDaemon::new().await;
    td.fake_subsonic.expect_starred().await;
    td.core.set_music_folder(Some(2)).await.unwrap();
    td.core.refresh_starred().await;

    let reqs = td.fake_subsonic.received_requests().await;
    let q = find(&reqs, "/rest/getStarred2")
        .url
        .query()
        .unwrap_or_default();
    assert!(
        q.contains("musicFolderId=2"),
        "the starred view must scope to the selected folder; query was {q}"
    );
}

#[tokio::test]
#[serial]
async fn no_folder_selected_omits_music_folder_id() {
    let td = TestDaemon::new().await;
    td.fake_subsonic.expect_artists(&["A"]).await;

    td.core.refresh_artists().await;

    let reqs = td.fake_subsonic.received_requests().await;
    let q = find(&reqs, "/rest/getArtists")
        .url
        .query()
        .unwrap_or_default();
    assert!(
        !q.contains("musicFolderId"),
        "with no folder selected, getArtists must browse all libraries; query was {q}"
    );
}

#[tokio::test]
#[serial]
async fn quick_play_album_uses_selected_music_folder() {
    let td = TestDaemon::new().await;
    td.fake_subsonic.expect_artists(&["A"]).await;
    td.fake_subsonic.expect_random_songs(&["s"]).await;
    td.core.set_music_folder(Some(2)).await.unwrap();
    td.fake_subsonic
        .expect_quick_play_album("newest", "new", "Newest", &["Track"])
        .await;

    td.core
        .refresh_quick_play_album(QuickPlayAlbumKind::Newest)
        .await;

    let requests = td.fake_subsonic.received_requests().await;
    let query = find(&requests, "/rest/getAlbumList2")
        .url
        .query()
        .unwrap_or_default();
    assert!(
        query.contains("type=newest") && query.contains("musicFolderId=2"),
        "curated albums must follow the active library; query was {query}"
    );
}

#[tokio::test]
#[serial]
async fn f_on_library_page_cycles_to_the_next_folder() {
    let td = TestDaemon::new().await;
    td.fake_subsonic.expect_artists(&["A"]).await;
    td.fake_subsonic.expect_random_songs(&["s"]).await;
    let cfg = td.state.read().await.config.clone();
    let mut app = App::with_remote_client(
        std::sync::Arc::new(ferrosonic::ipc::InProcessClient::new(td.core.clone())),
        cfg,
    );
    {
        let mut ds = app.daemon_state.write().await;
        ds.library.music_folders = vec![
            MusicFolder {
                id: 1,
                name: "Music".into(),
            },
            MusicFolder {
                id: 2,
                name: "Test".into(),
            },
        ];
        ds.config.music_folder_id = None;
    }

    app.handle_key(key(KeyCode::F(1))).await.unwrap();
    app.handle_key(key(KeyCode::Char('f'))).await.unwrap();

    let reqs = td.fake_subsonic.received_requests().await;
    let q = find(&reqs, "/rest/getArtists")
        .url
        .query()
        .unwrap_or_default();
    assert!(
        q.contains("musicFolderId=1"),
        "f cycles All -> the first folder (id 1) and rescopes browse; query was {q}"
    );
}

#[tokio::test]
#[serial]
async fn empty_library_clears_the_artist_tree_not_leaves_it_stale() {
    let td = TestDaemon::new().await;
    {
        let mut st = td.state.write().await;
        st.library.artists = vec![Artist {
            id: "a1".into(),
            name: "Old".into(),
            album_count: None,
            cover_art: None,
        }];
    }
    // Navidrome answers a scoped-empty library with error 70, not an empty index.
    td.fake_subsonic
        .expect_error("getArtists", 70, "Library not found or empty")
        .await;

    td.core.refresh_artists().await;

    let st = td.state.read().await;
    assert!(
        st.library.artists.is_empty(),
        "an empty library (error 70) clears the tree instead of leaving stale artists"
    );
}

#[tokio::test]
#[serial]
async fn first_run_defaults_to_the_servers_first_library_not_all() {
    let td = TestDaemon::new().await;
    td.fake_subsonic
        .expect_music_folders(&[(1, "Music Library"), (2, "test")])
        .await;
    td.fake_subsonic.expect_artists(&["A"]).await;
    td.fake_subsonic.expect_random_songs(&["s"]).await;

    // Fresh config: nothing chosen yet.
    td.core.refresh_music_folders().await;

    let st = td.state.read().await;
    assert_eq!(
        st.config.music_folder_id,
        Some(1),
        "with no prior choice, default to the first (default) library, not All"
    );
}

#[tokio::test]
#[serial]
async fn an_explicit_all_choice_is_not_overridden_by_the_default() {
    let td = TestDaemon::new().await;
    {
        let mut st = td.state.write().await;
        st.config.music_folder_id = None;
        st.config.music_folder_chosen = true; // user explicitly picked All
    }
    td.fake_subsonic
        .expect_music_folders(&[(1, "Music Library"), (2, "test")])
        .await;

    td.core.refresh_music_folders().await;

    let st = td.state.read().await;
    assert_eq!(
        st.config.music_folder_id, None,
        "an explicit All choice survives folder refreshes"
    );
}
