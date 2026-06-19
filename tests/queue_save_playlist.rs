//! Queue page: save-as-playlist (`s`) name box and createPlaylist request.

mod common;

use common::TestDaemon;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ferrosonic::app::App;
use ferrosonic::subsonic::models::Child;
use serial_test::serial;

fn key(code: KeyCode) -> KeyEvent {
    let mut k = KeyEvent::new(code, KeyModifiers::NONE);
    k.kind = KeyEventKind::Press;
    k
}

fn song(id: &str) -> Child {
    Child {
        id: id.into(),
        title: id.into(),
        parent: None,
        is_dir: false,
        album: None,
        artist: None,
        artist_id: None,
        album_id: None,
        track: None,
        year: None,
        genre: None,
        cover_art: None,
        size: None,
        content_type: None,
        suffix: None,
        duration: Some(180),
        bit_rate: None,
        path: None,
        disc_number: None,
        starred: None,
    }
}

async fn queue_app(songs: &[&str]) -> (App, TestDaemon) {
    let td = TestDaemon::new().await;
    let cfg = td.state.read().await.config.clone();
    let mut app = App::with_remote_client(
        std::sync::Arc::new(ferrosonic::ipc::InProcessClient::new(td.core.clone())),
        cfg,
    );
    {
        let mut ds = app.daemon_state.write().await;
        ds.queue = songs.iter().map(|s| song(s)).collect();
    }
    app.handle_key(key(KeyCode::F(2))).await.unwrap();
    (app, td)
}

async fn type_str(app: &mut App, s: &str) {
    for c in s.chars() {
        app.handle_key(key(KeyCode::Char(c))).await.unwrap();
    }
}

#[tokio::test]
#[serial]
async fn s_opens_the_name_box_when_queue_has_songs() {
    let (mut app, _td) = queue_app(&["q-0", "q-1"]).await;
    app.handle_key(key(KeyCode::Char('s'))).await.unwrap();
    let cs = app.client_state.read().await;
    assert!(
        cs.queue_state.naming_playlist,
        "s must open the save-as-playlist name box"
    );
}

#[tokio::test]
#[serial]
async fn s_on_empty_queue_does_not_open_the_box() {
    let (mut app, _td) = queue_app(&[]).await;
    app.handle_key(key(KeyCode::Char('s'))).await.unwrap();
    let cs = app.client_state.read().await;
    assert!(
        !cs.queue_state.naming_playlist,
        "an empty queue has nothing to save; the box must stay closed"
    );
}

#[tokio::test]
#[serial]
async fn esc_cancels_the_name_box_and_clears_typed_text() {
    let (mut app, _td) = queue_app(&["q-0"]).await;
    app.handle_key(key(KeyCode::Char('s'))).await.unwrap();
    type_str(&mut app, "Mix").await;
    app.handle_key(key(KeyCode::Esc)).await.unwrap();
    let cs = app.client_state.read().await;
    assert!(!cs.queue_state.naming_playlist, "Esc closes the box");
    assert!(
        cs.queue_state.playlist_name.is_empty(),
        "Esc discards the typed name"
    );
}

#[tokio::test]
#[serial]
async fn enter_with_empty_name_keeps_the_box_open() {
    let (mut app, _td) = queue_app(&["q-0"]).await;
    app.handle_key(key(KeyCode::Char('s'))).await.unwrap();
    app.handle_key(key(KeyCode::Enter)).await.unwrap();
    let cs = app.client_state.read().await;
    assert!(
        cs.queue_state.naming_playlist,
        "an empty name is rejected; the box stays open to retype"
    );
}

#[tokio::test]
#[serial]
async fn typing_a_name_and_enter_sends_createplaylist_with_the_queue_ids() {
    let (mut app, td) = queue_app(&["trk-1", "trk-2"]).await;
    td.fake_subsonic.expect_create_playlist().await;
    td.fake_subsonic.expect_playlists().await;

    app.handle_key(key(KeyCode::Char('s'))).await.unwrap();
    type_str(&mut app, "Road Trip").await;
    app.handle_key(key(KeyCode::Enter)).await.unwrap();

    {
        let cs = app.client_state.read().await;
        assert!(
            !cs.queue_state.naming_playlist,
            "the box closes once the playlist is saved"
        );
    }

    let reqs = td.fake_subsonic.received_requests().await;
    let create = reqs.iter().find(|r| r.url.path() == "/rest/createPlaylist");
    let Some(create) = create else {
        let paths: Vec<_> = reqs.iter().map(|r| r.url.path().to_string()).collect();
        panic!("no createPlaylist request was sent; saw {paths:?}");
    };
    let q = create.url.query().unwrap_or_default();
    assert!(
        q.contains("name=Road%20Trip") || q.contains("name=Road+Trip"),
        "the typed name is sent url-encoded; query was {q}"
    );
    assert!(
        q.contains("songId=trk-1") && q.contains("songId=trk-2"),
        "every queue song id is sent in order; query was {q}"
    );
}
