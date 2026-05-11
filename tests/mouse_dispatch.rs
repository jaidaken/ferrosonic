//! Mouse event dispatch through App::handle_mouse.

use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ferrosonic::app::App;
use ferrosonic::config::Config;
use ratatui::layout::Rect;
use serial_test::serial;

struct AppFixture {
    app: App,
    _tempdir: tempfile::TempDir,
}

async fn build_app() -> AppFixture {
    let tempdir = tempfile::tempdir().unwrap();
    std::env::set_var("FERROSONIC_CONFIG_DIR", tempdir.path());
    let mut config = Config::new();
    config.daemon = false;
    let app = App::new(config);
    AppFixture {
        app,
        _tempdir: tempdir,
    }
}

fn mouse(kind: MouseEventKind, x: u16, y: u16) -> MouseEvent {
    MouseEvent {
        kind,
        column: x,
        row: y,
        modifiers: KeyModifiers::NONE,
    }
}

async fn seed_layout(app: &App, header: Rect, content: Rect, now_playing: Rect) {
    let mut cs = app.client_state.write().await;
    cs.layout.header = header;
    cs.layout.content = content;
    cs.layout.now_playing = now_playing;
}

#[tokio::test]
#[serial]
async fn click_outside_any_region_is_safe() {
    let mut fx = build_app().await;
    seed_layout(
        &fx.app,
        Rect::new(0, 0, 80, 1),
        Rect::new(0, 1, 80, 20),
        Rect::new(0, 21, 80, 7),
    )
    .await;
    fx.app
        .handle_mouse(mouse(MouseEventKind::Down(MouseButton::Left), 999, 999))
        .await
        .unwrap();
}

#[tokio::test]
#[serial]
async fn scroll_up_in_content_area_is_safe() {
    let mut fx = build_app().await;
    seed_layout(
        &fx.app,
        Rect::new(0, 0, 80, 1),
        Rect::new(0, 1, 80, 20),
        Rect::new(0, 21, 80, 7),
    )
    .await;
    fx.app
        .handle_mouse(mouse(MouseEventKind::ScrollUp, 40, 10))
        .await
        .unwrap();
}

#[tokio::test]
#[serial]
async fn scroll_down_in_content_area_is_safe() {
    let mut fx = build_app().await;
    seed_layout(
        &fx.app,
        Rect::new(0, 0, 80, 1),
        Rect::new(0, 1, 80, 20),
        Rect::new(0, 21, 80, 7),
    )
    .await;
    fx.app
        .handle_mouse(mouse(MouseEventKind::ScrollDown, 40, 10))
        .await
        .unwrap();
}

#[tokio::test]
#[serial]
async fn right_click_is_no_op() {
    let mut fx = build_app().await;
    fx.app
        .handle_mouse(mouse(MouseEventKind::Down(MouseButton::Right), 10, 10))
        .await
        .unwrap();
}

#[tokio::test]
#[serial]
async fn middle_click_is_no_op() {
    let mut fx = build_app().await;
    fx.app
        .handle_mouse(mouse(MouseEventKind::Down(MouseButton::Middle), 10, 10))
        .await
        .unwrap();
}
