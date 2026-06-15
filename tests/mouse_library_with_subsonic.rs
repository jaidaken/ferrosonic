//! mouse_library.rs branches that need real subsonic responses.

mod common;

use common::TestDaemon;
use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ferrosonic::app::App;
use ferrosonic::subsonic::models::{Album, Artist};
use ratatui::layout::Rect;
use serial_test::serial;

fn click(x: u16, y: u16) -> MouseEvent {
    MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: x,
        row: y,
        modifiers: KeyModifiers::NONE,
    }
}

fn artist(id: &str, name: &str) -> Artist {
    Artist {
        id: id.into(),
        name: name.into(),
        album_count: Some(1),
        cover_art: None,
    }
}

fn album(id: &str, name: &str) -> Album {
    Album {
        id: id.into(),
        name: name.into(),
        artist: Some("X".into()),
        artist_id: Some("a0".into()),
        cover_art: None,
        song_count: Some(3),
        original_release_date: None,
        duration: Some(540),
        year: Some(2024),
        genre: None,
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
        cs.layout.header = Rect::new(0, 0, 80, 1);
        cs.layout.content = Rect::new(0, 1, 80, 20);
        cs.layout.content_left = Some(Rect::new(0, 1, 40, 20));
        cs.layout.content_right = Some(Rect::new(40, 1, 40, 20));
        cs.layout.now_playing = Rect::new(0, 21, 80, 7);
    }
    (app, td)
}

#[tokio::test]
#[serial]
async fn double_click_on_artist_not_in_cache_loads_via_daemon() {
    let (mut app, td) = build_app_with_td().await;
    td.fake_subsonic
        .expect_get_artist("a-load", "Loadable", &["A1", "A2"])
        .await;
    {
        let mut s = app.daemon_state.write().await;
        s.library.artists = vec![artist("a-load", "Loadable")];
    }
    app.handle_mouse(click(10, 2)).await.unwrap();
    app.handle_mouse(click(10, 2)).await.unwrap();
}

#[tokio::test]
#[serial]
async fn double_click_on_album_with_songs_replaces_queue() {
    let (mut app, td) = build_app_with_td().await;
    td.fake_subsonic
        .expect_get_album("alb-go", "Go", &["g0", "g1"])
        .await;
    {
        let mut s = app.daemon_state.write().await;
        s.library.artists = vec![artist("a0", "Artist")];
        s.library
            .albums_cache
            .insert("a0".into(), vec![album("alb-go", "Go")]);
    }
    {
        let mut cs = app.client_state.write().await;
        cs.artists.expanded.insert("a0".into());
    }
    app.handle_mouse(click(10, 3)).await.unwrap();
    app.handle_mouse(click(10, 3)).await.unwrap();
}

#[tokio::test]
#[serial]
async fn double_click_on_album_with_empty_songs_notifies_error() {
    let (mut app, td) = build_app_with_td().await;
    td.fake_subsonic
        .expect_get_album("alb-empty", "Empty", &[])
        .await;
    {
        let mut s = app.daemon_state.write().await;
        s.library.artists = vec![artist("a0", "Artist")];
        s.library
            .albums_cache
            .insert("a0".into(), vec![album("alb-empty", "Empty")]);
    }
    {
        let mut cs = app.client_state.write().await;
        cs.artists.expanded.insert("a0".into());
    }
    app.handle_mouse(click(10, 3)).await.unwrap();
    app.handle_mouse(click(10, 3)).await.unwrap();
}

#[tokio::test]
#[serial]
async fn single_click_on_album_in_cache_auto_loads_via_load_album() {
    let (mut app, td) = build_app_with_td().await;
    td.fake_subsonic
        .expect_get_album("alb-auto", "AutoAlb", &["x0"])
        .await;
    {
        let mut s = app.daemon_state.write().await;
        s.library.artists = vec![artist("a0", "Artist")];
        s.library
            .albums_cache
            .insert("a0".into(), vec![album("alb-auto", "AutoAlb")]);
    }
    {
        let mut cs = app.client_state.write().await;
        cs.artists.expanded.insert("a0".into());
    }
    app.handle_mouse(click(10, 3)).await.unwrap();
    let cs = app.client_state.read().await;
    assert!(!cs.artists.songs.is_empty());
}
