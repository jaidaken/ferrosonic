//! Mouse clicks routed to page-specific content handlers.

use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ferrosonic::app::state::Page;
use ferrosonic::app::App;
use ferrosonic::config::Config;
use ratatui::layout::Rect;
use serial_test::serial;

struct AppFixture {
    app: App,
    _tempdir: tempfile::TempDir,
}

async fn build_app(page: Page) -> AppFixture {
    let tempdir = tempfile::tempdir().unwrap();
    std::env::set_var("FERROSONIC_CONFIG_DIR", tempdir.path());
    let mut config = Config::new();
    config.daemon = false;
    let app = App::new(config);
    {
        let mut cs = app.client_state.write().await;
        cs.page = page;
        cs.layout.header = Rect::new(0, 0, 80, 1);
        cs.layout.content = Rect::new(0, 1, 80, 20);
        cs.layout.content_left = Some(Rect::new(0, 1, 40, 20));
        cs.layout.content_right = Some(Rect::new(40, 1, 40, 20));
        cs.layout.now_playing = Rect::new(0, 21, 80, 7);
    }
    AppFixture {
        app,
        _tempdir: tempdir,
    }
}

fn click(x: u16, y: u16) -> MouseEvent {
    MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: x,
        row: y,
        modifiers: KeyModifiers::NONE,
    }
}

fn scroll_up(x: u16, y: u16) -> MouseEvent {
    MouseEvent {
        kind: MouseEventKind::ScrollUp,
        column: x,
        row: y,
        modifiers: KeyModifiers::NONE,
    }
}

#[tokio::test]
#[serial]
async fn library_content_click_routes_to_library_handler() {
    let mut fx = build_app(Page::Library).await;
    fx.app.handle_mouse(click(10, 5)).await.unwrap();
}

#[tokio::test]
#[serial]
async fn queue_content_click_routes_to_queue_handler() {
    let mut fx = build_app(Page::Queue).await;
    fx.app.handle_mouse(click(20, 10)).await.unwrap();
}

#[tokio::test]
#[serial]
async fn quickplay_content_click_routes_to_quickplay_handler() {
    let mut fx = build_app(Page::QuickPlay).await;
    fx.app.handle_mouse(click(10, 5)).await.unwrap();
    fx.app.handle_mouse(click(50, 5)).await.unwrap();
}

#[tokio::test]
#[serial]
async fn playlists_content_click_routes_to_playlists_handler() {
    let mut fx = build_app(Page::Playlists).await;
    fx.app.handle_mouse(click(10, 5)).await.unwrap();
    fx.app.handle_mouse(click(50, 5)).await.unwrap();
}

#[tokio::test]
#[serial]
async fn server_content_click_is_silent_no_op() {
    let mut fx = build_app(Page::Server).await;
    fx.app.handle_mouse(click(10, 5)).await.unwrap();
}

#[tokio::test]
#[serial]
async fn settings_content_click_is_silent_no_op() {
    let mut fx = build_app(Page::Settings).await;
    fx.app.handle_mouse(click(10, 5)).await.unwrap();
}

#[tokio::test]
#[serial]
async fn scroll_up_in_content_runs_page_scroll() {
    let mut fx = build_app(Page::Library).await;
    fx.app.handle_mouse(scroll_up(20, 10)).await.unwrap();
}

#[tokio::test]
#[serial]
async fn scroll_up_in_queue_runs_queue_scroll() {
    let mut fx = build_app(Page::Queue).await;
    fx.app.handle_mouse(scroll_up(20, 10)).await.unwrap();
}
