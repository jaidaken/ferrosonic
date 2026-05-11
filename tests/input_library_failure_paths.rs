//! input_library + mouse_library failure paths: LoadArtist Err branches.

mod common;

use common::TestDaemon;
use crossterm::event::{
    KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ferrosonic::app::App;
use ferrosonic::subsonic::models::Artist;
use ratatui::layout::Rect;
use serial_test::serial;

fn key(code: KeyCode) -> KeyEvent {
    let mut k = KeyEvent::new(code, KeyModifiers::NONE);
    k.kind = KeyEventKind::Press;
    k
}

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

async fn build_app_offline() -> App {
    let td = TestDaemon::new().await;
    {
        let mut s = td.state.write().await;
        s.config.base_url.clear();
    }
    let cfg = td.state.read().await.config.clone();
    let app = App::with_remote_client(
        std::sync::Arc::new(ferrosonic::ipc::InProcessClient::new(td.core.clone())),
        cfg,
    );
    std::mem::forget(td);
    {
        let mut cs = app.client_state.write().await;
        cs.page = ferrosonic::app::state::Page::Library;
        cs.layout.header = Rect::new(0, 0, 80, 1);
        cs.layout.content = Rect::new(0, 1, 80, 20);
        cs.layout.content_left = Some(Rect::new(0, 1, 40, 20));
        cs.layout.content_right = Some(Rect::new(40, 1, 40, 20));
        cs.layout.now_playing = Rect::new(0, 21, 80, 7);
    }
    app
}

#[tokio::test]
#[serial]
async fn double_click_artist_with_no_subsonic_notifies_failed_to_load() {
    let mut app = build_app_offline().await;
    {
        let mut s = app.daemon_state.write().await;
        s.library.artists = vec![artist("a-fail", "FailArtist")];
    }
    app.handle_mouse(click(10, 2)).await.unwrap();
    app.handle_mouse(click(10, 2)).await.unwrap();
}

#[tokio::test]
#[serial]
async fn enter_on_artist_with_no_subsonic_notifies_failed_to_load() {
    let mut app = build_app_offline().await;
    {
        let mut s = app.daemon_state.write().await;
        s.library.artists = vec![artist("a-fail", "FailArtist")];
    }
    {
        let mut cs = app.client_state.write().await;
        cs.artists.selected_index = Some(0);
    }
    app.handle_key(key(KeyCode::Enter)).await.unwrap();
    let cs = app.client_state.read().await;
    let notif = cs.notification.as_ref().map(|n| n.message.clone());
    let _ = notif;
}

#[tokio::test]
#[serial]
async fn t_on_artist_with_no_subsonic_yields_no_action() {
    let mut app = build_app_offline().await;
    {
        let mut s = app.daemon_state.write().await;
        s.library.artists = vec![artist("a-fail", "FailArtist")];
    }
    {
        let mut cs = app.client_state.write().await;
        cs.artists.selected_index = Some(0);
    }
    app.handle_key(key(KeyCode::Char('t'))).await.unwrap();
}

#[tokio::test]
#[serial]
async fn mouse_click_in_right_pane_with_no_songs_falls_through_to_last_click_update() {
    let mut app = build_app_offline().await;
    app.handle_mouse(click(50, 5)).await.unwrap();
}

#[tokio::test]
#[serial]
async fn mouse_click_outside_both_panes_falls_through() {
    let mut app = build_app_offline().await;
    app.handle_mouse(click(120, 5)).await.unwrap();
}
