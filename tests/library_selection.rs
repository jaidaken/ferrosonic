//! Library (music folder) selection: getMusicFolders populates state, and the
//! selected folder scopes browse calls via musicFolderId.

mod common;

use common::TestDaemon;
use serial_test::serial;
use wiremock::Request;

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
