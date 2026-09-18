//! Artist/album info overlay: rendering, open/close keys, scrolling.

mod common;

use std::sync::Arc;

use common::{render, RecordingClient};
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ferrosonic::app::page_state::{InfoKind, InfoPayload, InfoStatus};
use ferrosonic::app::state::Page;
use ferrosonic::app::App;
use ferrosonic::config::Config;
use ferrosonic::daemon::DaemonState;
use ferrosonic::ipc::client::DaemonClient;
use ferrosonic::subsonic::models::{Artist, ArtistInfo2};
use serde_json::json;
use serial_test::serial;

fn key(code: KeyCode) -> KeyEvent {
    let mut k = KeyEvent::new(code, KeyModifiers::NONE);
    k.kind = KeyEventKind::Press;
    k
}

fn base_state() -> (DaemonState, ferrosonic::app::client_state::ClientState) {
    let mut daemon = DaemonState::new(Config::new());
    daemon.library.artists = vec![Artist {
        id: "a1".into(),
        name: "The Cure".into(),
        album_count: Some(13),
        cover_art: None,
    }];
    let mut client = ferrosonic::app::client_state::ClientState::default();
    client.page = Page::Library;
    (daemon, client)
}

#[test]
fn ready_artist_info_renders_biography_and_similar_artists() {
    let (daemon, mut client) = base_state();
    client.artists.selected_index = Some(0);
    client.info.open = true;
    client.info.kind = InfoKind::Artist;
    client.info.title = "The Cure".into();
    client.info.status = InfoStatus::Ready(InfoPayload::Artist(ArtistInfo2 {
        biography: Some("An English rock band formed in 1978.".into()),
        similar_artist: vec![Artist {
            id: "a2".into(),
            name: "The Smiths".into(),
            album_count: None,
            cover_art: None,
        }],
        last_fm_url: Some("https://last.fm/music/The+Cure".into()),
        ..Default::default()
    }));

    let frame = render(120, 30, &daemon, &mut client);
    assert!(
        frame.contains("An English rock band"),
        "biography must render;\n{frame}"
    );
    assert!(
        frame.contains("The Smiths"),
        "similar artists must render;\n{frame}"
    );
    assert!(
        frame.contains("Last.fm"),
        "external link must render;\n{frame}"
    );
}

#[test]
fn empty_info_renders_the_graceful_empty_state() {
    let (daemon, mut client) = base_state();
    client.info.open = true;
    client.info.kind = InfoKind::Album;
    client.info.title = "Unknown".into();
    client.info.status = InfoStatus::Empty;

    let frame = render(120, 30, &daemon, &mut client);
    assert!(
        frame.contains("No information available"),
        "an empty result must show a clean empty state, not an error;\n{frame}"
    );
}

#[tokio::test]
#[serial]
async fn info_key_opens_for_the_selected_artist_and_escape_closes() {
    let dir = common::tempdir();
    std::env::set_var("FERROSONIC_CONFIG_DIR", dir.path());
    let client = RecordingClient::new();
    let client_dyn: Arc<dyn DaemonClient> = client.clone();
    let mut app = App::with_remote_client(client_dyn, Config::new());
    {
        let mut ds = app.daemon_state.write().await;
        ds.library.artists = vec![Artist {
            id: "a1".into(),
            name: "The Cure".into(),
            album_count: Some(13),
            cover_art: None,
        }];
    }
    {
        let mut cs = app.client_state.write().await;
        cs.page = Page::Library;
        cs.artists.selected_index = Some(0);
        cs.artists.focus = 0;
    }

    app.handle_key(key(KeyCode::Char('I'))).await.unwrap();
    {
        let cs = app.client_state.read().await;
        assert!(cs.info.open, "I must open the info overlay");
        assert_eq!(cs.info.target_id.as_deref(), Some("a1"));
        assert_eq!(cs.info.kind, InfoKind::Artist);
    }

    app.handle_key(key(KeyCode::Esc)).await.unwrap();
    assert!(
        !app.client_state.read().await.info.open,
        "Escape must close the overlay"
    );
}

#[tokio::test]
#[serial]
async fn info_scroll_keys_move_within_the_body() {
    let dir = common::tempdir();
    std::env::set_var("FERROSONIC_CONFIG_DIR", dir.path());
    let client = RecordingClient::new();
    let client_dyn: Arc<dyn DaemonClient> = client.clone();
    let mut app = App::with_remote_client(client_dyn, Config::new());
    {
        let mut cs = app.client_state.write().await;
        cs.info.open = true;
        cs.info.status = InfoStatus::Ready(InfoPayload::Album(
            ferrosonic::subsonic::models::AlbumInfo {
                notes: Some("notes".into()),
                ..Default::default()
            },
        ));
        cs.info.scroll = 0;
    }

    app.handle_key(key(KeyCode::Char('j'))).await.unwrap();
    assert_eq!(app.client_state.read().await.info.scroll, 1);
    app.handle_key(key(KeyCode::PageDown)).await.unwrap();
    assert_eq!(app.client_state.read().await.info.scroll, 11);
    app.handle_key(key(KeyCode::Char('k'))).await.unwrap();
    assert_eq!(app.client_state.read().await.info.scroll, 10);
    app.handle_key(key(KeyCode::PageUp)).await.unwrap();
    assert_eq!(app.client_state.read().await.info.scroll, 0);
    // Never underflows.
    app.handle_key(key(KeyCode::Char('k'))).await.unwrap();
    assert_eq!(app.client_state.read().await.info.scroll, 0);
}

#[test]
fn info_payload_serializes_for_the_wire() {
    // The IPC response box carries these, so they must round-trip JSON.
    let info = ArtistInfo2 {
        biography: Some("bio".into()),
        last_fm_url: Some("https://last.fm/x".into()),
        ..Default::default()
    };
    let value = serde_json::to_value(&info).unwrap();
    assert_eq!(value["biography"], json!("bio"));
}
