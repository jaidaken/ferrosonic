//! Library (music folder) selection: getMusicFolders populates state, and the
//! selected folder scopes browse calls via musicFolderId.

mod common;

use common::TestDaemon;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ferrosonic::app::App;
use ferrosonic::subsonic::models::MusicFolder;
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
