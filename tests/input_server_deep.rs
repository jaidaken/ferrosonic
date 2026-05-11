//! Server page input: field nav, text input, Enter for test connection.

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ferrosonic::app::App;
use ferrosonic::config::Config;
use serial_test::serial;

fn key(code: KeyCode) -> KeyEvent {
    let mut k = KeyEvent::new(code, KeyModifiers::NONE);
    k.kind = KeyEventKind::Press;
    k
}

struct AppFixture {
    app: App,
    _tempdir: tempfile::TempDir,
}

async fn build_app() -> AppFixture {
    let tempdir = tempfile::tempdir().unwrap();
    std::env::set_var("FERROSONIC_CONFIG_DIR", tempdir.path());
    let mut config = Config::new();
    config.daemon = false;
    let mut app = App::new(config);
    app.handle_key(key(KeyCode::F(5))).await.unwrap();
    AppFixture {
        app,
        _tempdir: tempdir,
    }
}

#[tokio::test]
#[serial]
async fn down_advances_through_server_fields() {
    let mut fx = build_app().await;
    for _ in 0..10 {
        fx.app.handle_key(key(KeyCode::Down)).await.unwrap();
    }
    let cs = fx.app.client_state.read().await;
    assert_eq!(cs.server_state.selected_field, 4, "Down caps at field 4");
}

#[tokio::test]
#[serial]
async fn tab_cycles_fields() {
    let mut fx = build_app().await;
    let initial = fx.app.client_state.read().await.server_state.selected_field;
    fx.app.handle_key(key(KeyCode::Tab)).await.unwrap();
    let after = fx.app.client_state.read().await.server_state.selected_field;
    assert_ne!(initial, after);
}

#[tokio::test]
#[serial]
async fn typing_appends_to_url_field() {
    let mut fx = build_app().await;
    for c in "test".chars() {
        fx.app.handle_key(key(KeyCode::Char(c))).await.unwrap();
    }
    let cs = fx.app.client_state.read().await;
    assert!(cs.server_state.base_url.contains("test"));
}

#[tokio::test]
#[serial]
async fn backspace_trims_url_field() {
    let mut fx = build_app().await;
    for c in "abc".chars() {
        fx.app.handle_key(key(KeyCode::Char(c))).await.unwrap();
    }
    fx.app.handle_key(key(KeyCode::Backspace)).await.unwrap();
    let cs = fx.app.client_state.read().await;
    assert_eq!(cs.server_state.base_url, "ab");
}

#[tokio::test]
#[serial]
async fn tab_to_username_field_typing_appends_to_username() {
    let mut fx = build_app().await;
    fx.app.handle_key(key(KeyCode::Down)).await.unwrap();
    for c in "user".chars() {
        fx.app.handle_key(key(KeyCode::Char(c))).await.unwrap();
    }
    let cs = fx.app.client_state.read().await;
    assert_eq!(cs.server_state.username, "user");
}
