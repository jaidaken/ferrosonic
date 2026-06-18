//! More input_library.rs branches: empty search-result handling + 'i'/'m' edges.

mod common;

use common::TestDaemon;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ferrosonic::app::App;
use ferrosonic::subsonic::models::{Album, Child, SearchResult3};
use serial_test::serial;

fn key(code: KeyCode) -> KeyEvent {
    let mut k = KeyEvent::new(code, KeyModifiers::NONE);
    k.kind = KeyEventKind::Press;
    k
}

fn album(id: &str, name: &str) -> Album {
    Album {
        id: id.into(),
        name: name.into(),
        artist: Some("X".into()),
        artist_id: Some("a0".into()),
        cover_art: None,
        song_count: Some(0),
        original_release_date: None,
        duration: Some(0),
        year: Some(2024),
        genre: None,
    }
}

fn song(id: &str) -> Child {
    Child {
        id: id.into(),
        title: id.into(),
        parent: None,
        is_dir: false,
        album: None,
        artist: None,
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

async fn build_app_with_td() -> (App, TestDaemon) {
    let td = TestDaemon::new().await;
    let cfg = td.state.read().await.config.clone();
    let app = App::with_remote_client(
        std::sync::Arc::new(ferrosonic::ipc::InProcessClient::new(td.core.clone())),
        cfg,
    );
    {
        let mut cs = app.client_state.write().await;
        cs.page = ferrosonic::app::state::Page::Library;
    }
    (app, td)
}

#[tokio::test]
#[serial]
async fn e_in_search_mode_for_empty_album_collects_nothing_and_does_not_enqueue() {
    let (mut app, td) = build_app_with_td().await;
    td.fake_subsonic
        .expect_get_album("alb-empty", "Empty", &[])
        .await;
    {
        let mut cs = app.client_state.write().await;
        cs.artists.filter = "e".into();
        cs.artists.search_results = Some(SearchResult3 {
            artist: vec![],
            album: vec![album("alb-empty", "Empty")],
            song: vec![],
        });
        cs.artists.selected_index = Some(0);
    }
    app.handle_key(key(KeyCode::Char('e'))).await.unwrap();
    let ds = td.state.read().await;
    assert!(ds.queue.is_empty());
}

#[tokio::test]
#[serial]
async fn i_in_search_mode_for_empty_album_collects_nothing() {
    let (mut app, td) = build_app_with_td().await;
    td.fake_subsonic
        .expect_get_album("alb-empty2", "Empty2", &[])
        .await;
    {
        let mut cs = app.client_state.write().await;
        cs.artists.filter = "x".into();
        cs.artists.search_results = Some(SearchResult3 {
            artist: vec![],
            album: vec![album("alb-empty2", "Empty2")],
            song: vec![],
        });
        cs.artists.selected_index = Some(0);
    }
    app.handle_key(key(KeyCode::Char('i'))).await.unwrap();
    let ds = td.state.read().await;
    assert!(ds.queue.is_empty());
}

#[tokio::test]
#[serial]
async fn e_in_song_pane_with_no_selection_is_noop() {
    let (mut app, _td) = build_app_with_td().await;
    {
        let mut cs = app.client_state.write().await;
        cs.artists.focus = 1;
        cs.artists.songs = vec![song("a")];
    }
    app.handle_key(key(KeyCode::Char('e'))).await.unwrap();
}

#[tokio::test]
#[serial]
async fn i_in_song_pane_with_no_selection_is_noop() {
    let (mut app, _td) = build_app_with_td().await;
    {
        let mut cs = app.client_state.write().await;
        cs.artists.focus = 1;
        cs.artists.songs = vec![song("a")];
    }
    app.handle_key(key(KeyCode::Char('i'))).await.unwrap();
}

#[tokio::test]
#[serial]
async fn m_in_song_pane_with_no_selection_is_noop() {
    let (mut app, _td) = build_app_with_td().await;
    {
        let mut cs = app.client_state.write().await;
        cs.artists.focus = 1;
        cs.artists.songs = vec![song("a")];
    }
    app.handle_key(key(KeyCode::Char('m'))).await.unwrap();
}

#[tokio::test]
#[serial]
async fn t_in_search_mode_with_no_selection_is_noop() {
    let (mut app, _td) = build_app_with_td().await;
    {
        let mut cs = app.client_state.write().await;
        cs.artists.filter = "x".into();
        cs.artists.search_results = Some(SearchResult3 {
            artist: vec![],
            album: vec![],
            song: vec![song("a")],
        });
    }
    app.handle_key(key(KeyCode::Char('t'))).await.unwrap();
}

#[tokio::test]
#[serial]
async fn down_arrow_on_empty_song_pane_initializes_none() {
    let (mut app, _td) = build_app_with_td().await;
    {
        let mut cs = app.client_state.write().await;
        cs.artists.focus = 1;
    }
    app.handle_key(key(KeyCode::Down)).await.unwrap();
}

#[tokio::test]
#[serial]
async fn up_arrow_on_empty_song_pane_initializes_none() {
    let (mut app, _td) = build_app_with_td().await;
    {
        let mut cs = app.client_state.write().await;
        cs.artists.focus = 1;
    }
    app.handle_key(key(KeyCode::Up)).await.unwrap();
}

#[tokio::test]
#[serial]
async fn enter_on_song_pane_with_no_selection_is_noop() {
    let (mut app, _td) = build_app_with_td().await;
    {
        let mut cs = app.client_state.write().await;
        cs.artists.focus = 1;
        cs.artists.songs = vec![song("a")];
    }
    app.handle_key(key(KeyCode::Enter)).await.unwrap();
}

#[tokio::test]
#[serial]
async fn enter_in_tree_with_no_selection_is_noop() {
    let (mut app, _td) = build_app_with_td().await;
    app.handle_key(key(KeyCode::Enter)).await.unwrap();
}
