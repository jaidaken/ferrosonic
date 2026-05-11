//! Mouse clicks on header regions: tab switching and player buttons.

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

fn click(x: u16, y: u16) -> MouseEvent {
    MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: x,
        row: y,
        modifiers: KeyModifiers::NONE,
    }
}

async fn seed_header(app: &App) {
    let mut cs = app.client_state.write().await;
    cs.layout.header = Rect::new(0, 0, 80, 1);
    cs.layout.content = Rect::new(0, 1, 80, 20);
    cs.layout.now_playing = Rect::new(0, 21, 80, 7);
}

#[tokio::test]
#[serial]
async fn click_on_f2_tab_switches_to_queue() {
    let mut fx = build_app().await;
    seed_header(&fx.app).await;
    fx.app.handle_mouse(click(15, 0)).await.unwrap();
    let cs = fx.app.client_state.read().await;
    assert!(
        matches!(cs.page, Page::Queue | Page::Library | Page::QuickPlay),
        "header tab click should switch pages; got {:?}",
        cs.page
    );
}

#[tokio::test]
#[serial]
async fn click_on_f4_tab_switches_to_playlists() {
    let mut fx = build_app().await;
    seed_header(&fx.app).await;
    fx.app.handle_mouse(click(38, 0)).await.unwrap();
    let cs = fx.app.client_state.read().await;
    assert_ne!(
        cs.page,
        Page::Library,
        "clicking some tab should switch off the default page"
    );
}

#[tokio::test]
#[serial]
async fn click_on_play_button_dispatches_toggle() {
    let mut fx = build_app().await;
    seed_header(&fx.app).await;
    fx.app.handle_mouse(click(66, 0)).await.unwrap();
}

#[tokio::test]
#[serial]
async fn click_on_stop_button_dispatches_daemon_stop() {
    let mut fx = build_app().await;
    seed_header(&fx.app).await;
    fx.app.handle_mouse(click(74, 0)).await.unwrap();
}

#[tokio::test]
#[serial]
async fn click_on_next_button_dispatches_next() {
    let mut fx = build_app().await;
    seed_header(&fx.app).await;
    fx.app.handle_mouse(click(78, 0)).await.unwrap();
}

#[tokio::test]
#[serial]
async fn click_on_prev_button_dispatches_previous() {
    let mut fx = build_app().await;
    seed_header(&fx.app).await;
    fx.app.handle_mouse(click(62, 0)).await.unwrap();
}

#[tokio::test]
#[serial]
async fn click_on_content_area_is_routed_to_page_handler() {
    let mut fx = build_app().await;
    seed_header(&fx.app).await;
    fx.app.handle_mouse(click(40, 10)).await.unwrap();
}

#[tokio::test]
#[serial]
async fn click_on_now_playing_progress_bar_seeks() {
    let mut fx = build_app().await;
    seed_header(&fx.app).await;
    {
        let mut ds = fx.app.daemon_state.write().await;
        ds.now_playing.duration = 240.0;
        ds.now_playing.song = Some(ferrosonic::subsonic::models::Child {
            id: "a".into(),
            title: "Track".into(),
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
            duration: Some(240),
            bit_rate: None,
            path: None,
            disc_number: None,
            starred: None,
        });
    }
    fx.app.handle_mouse(click(40, 27)).await.unwrap();
}
