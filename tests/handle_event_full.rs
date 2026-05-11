//! app/input.rs: handle_event paths (Resize + Mouse + non-press keys).

use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ferrosonic::app::App;
use ferrosonic::config::Config;
use serial_test::serial;

async fn build_app() -> App {
    let tempdir = tempfile::tempdir().unwrap();
    std::env::set_var("FERROSONIC_CONFIG_DIR", tempdir.path());
    std::mem::forget(tempdir);
    let mut config = Config::new();
    config.daemon = false;
    App::new(config)
}

#[tokio::test]
#[serial]
async fn resize_event_with_no_cava_is_noop() {
    let mut app = build_app().await;
    app.handle_event(Event::Resize(120, 40)).await.unwrap();
}

#[tokio::test]
#[serial]
async fn mouse_event_routes_to_handle_mouse() {
    let mut app = build_app().await;
    app.handle_event(Event::Mouse(MouseEvent {
        kind: MouseEventKind::ScrollUp,
        column: 0,
        row: 0,
        modifiers: KeyModifiers::NONE,
    }))
    .await
    .unwrap();
}

#[tokio::test]
#[serial]
async fn mouse_click_event_routes_to_handler() {
    let mut app = build_app().await;
    app.handle_event(Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 5,
        row: 5,
        modifiers: KeyModifiers::NONE,
    }))
    .await
    .unwrap();
}

#[tokio::test]
#[serial]
async fn key_release_event_is_filtered() {
    let mut app = build_app().await;
    let mut k = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE);
    k.kind = KeyEventKind::Release;
    app.handle_event(Event::Key(k)).await.unwrap();
    assert!(!app.client_state.read().await.should_quit);
}

#[tokio::test]
#[serial]
async fn key_repeat_event_is_filtered() {
    let mut app = build_app().await;
    let mut k = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE);
    k.kind = KeyEventKind::Repeat;
    app.handle_event(Event::Key(k)).await.unwrap();
    assert!(!app.client_state.read().await.should_quit);
}

#[tokio::test]
#[serial]
async fn key_press_event_does_route() {
    let mut app = build_app().await;
    let mut k = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE);
    k.kind = KeyEventKind::Press;
    app.handle_event(Event::Key(k)).await.unwrap();
    assert!(app.client_state.read().await.should_quit);
}

#[tokio::test]
#[serial]
async fn focus_gained_event_is_noop() {
    let mut app = build_app().await;
    app.handle_event(Event::FocusGained).await.unwrap();
}

#[tokio::test]
#[serial]
async fn focus_lost_event_is_noop() {
    let mut app = build_app().await;
    app.handle_event(Event::FocusLost).await.unwrap();
}

#[tokio::test]
#[serial]
async fn paste_event_is_noop() {
    let mut app = build_app().await;
    app.handle_event(Event::Paste("hello".into()))
        .await
        .unwrap();
}
