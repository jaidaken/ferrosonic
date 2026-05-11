//! Exhaustive input_server.rs branches.

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
async fn up_at_top_field_zero_stays() {
    let mut fx = build_app().await;
    {
        let mut cs = fx.app.client_state.write().await;
        cs.server_state.selected_field = 0;
    }
    fx.app.handle_key(key(KeyCode::Up)).await.unwrap();
    assert_eq!(
        fx.app.client_state.read().await.server_state.selected_field,
        0
    );
}

#[tokio::test]
#[serial]
async fn down_at_field_four_stays() {
    let mut fx = build_app().await;
    {
        let mut cs = fx.app.client_state.write().await;
        cs.server_state.selected_field = 4;
    }
    fx.app.handle_key(key(KeyCode::Down)).await.unwrap();
    assert_eq!(
        fx.app.client_state.read().await.server_state.selected_field,
        4
    );
}

#[tokio::test]
#[serial]
async fn tab_cycles_field_through_five() {
    let mut fx = build_app().await;
    {
        let mut cs = fx.app.client_state.write().await;
        cs.server_state.selected_field = 4;
    }
    fx.app.handle_key(key(KeyCode::Tab)).await.unwrap();
    assert_eq!(
        fx.app.client_state.read().await.server_state.selected_field,
        0
    );
}

#[tokio::test]
#[serial]
async fn char_into_url_field_appends() {
    let mut fx = build_app().await;
    {
        let mut cs = fx.app.client_state.write().await;
        cs.server_state.selected_field = 0;
        cs.server_state.base_url.clear();
    }
    fx.app.handle_key(key(KeyCode::Char('a'))).await.unwrap();
    fx.app.handle_key(key(KeyCode::Char('b'))).await.unwrap();
    assert_eq!(fx.app.client_state.read().await.server_state.base_url, "ab");
}

#[tokio::test]
#[serial]
async fn char_into_username_field_appends() {
    let mut fx = build_app().await;
    {
        let mut cs = fx.app.client_state.write().await;
        cs.server_state.selected_field = 1;
        cs.server_state.username.clear();
    }
    fx.app.handle_key(key(KeyCode::Char('u'))).await.unwrap();
    assert_eq!(fx.app.client_state.read().await.server_state.username, "u");
}

#[tokio::test]
#[serial]
async fn char_into_password_field_appends() {
    let mut fx = build_app().await;
    {
        let mut cs = fx.app.client_state.write().await;
        cs.server_state.selected_field = 2;
        cs.server_state.password.clear();
    }
    fx.app.handle_key(key(KeyCode::Char('p'))).await.unwrap();
    assert_eq!(fx.app.client_state.read().await.server_state.password, "p");
}

#[tokio::test]
#[serial]
async fn backspace_pops_from_url() {
    let mut fx = build_app().await;
    {
        let mut cs = fx.app.client_state.write().await;
        cs.server_state.selected_field = 0;
        cs.server_state.base_url = "abc".into();
    }
    fx.app.handle_key(key(KeyCode::Backspace)).await.unwrap();
    assert_eq!(fx.app.client_state.read().await.server_state.base_url, "ab");
}

#[tokio::test]
#[serial]
async fn backspace_pops_from_username() {
    let mut fx = build_app().await;
    {
        let mut cs = fx.app.client_state.write().await;
        cs.server_state.selected_field = 1;
        cs.server_state.username = "user".into();
    }
    fx.app.handle_key(key(KeyCode::Backspace)).await.unwrap();
    assert_eq!(
        fx.app.client_state.read().await.server_state.username,
        "use"
    );
}

#[tokio::test]
#[serial]
async fn backspace_pops_from_password() {
    let mut fx = build_app().await;
    {
        let mut cs = fx.app.client_state.write().await;
        cs.server_state.selected_field = 2;
        cs.server_state.password = "secret".into();
    }
    fx.app.handle_key(key(KeyCode::Backspace)).await.unwrap();
    assert_eq!(
        fx.app.client_state.read().await.server_state.password,
        "secre"
    );
}

#[tokio::test]
#[serial]
async fn enter_on_text_field_is_ignored() {
    let mut fx = build_app().await;
    {
        let mut cs = fx.app.client_state.write().await;
        cs.server_state.selected_field = 0;
        cs.server_state.base_url = "x".into();
    }
    fx.app.handle_key(key(KeyCode::Enter)).await.unwrap();
    assert_eq!(fx.app.client_state.read().await.server_state.base_url, "x");
}

#[tokio::test]
#[serial]
async fn enter_on_test_connection_field_sets_status() {
    let mut fx = build_app().await;
    {
        let mut cs = fx.app.client_state.write().await;
        cs.server_state.selected_field = 3;
    }
    fx.app.handle_key(key(KeyCode::Enter)).await.unwrap();
    let cs = fx.app.client_state.read().await;
    assert!(cs.server_state.status.is_some());
}

#[tokio::test]
#[serial]
async fn enter_on_save_field_sets_status() {
    let mut fx = build_app().await;
    {
        let mut cs = fx.app.client_state.write().await;
        cs.server_state.selected_field = 4;
        cs.server_state.base_url = "https://example.com".into();
        cs.server_state.username = "u".into();
        cs.server_state.password = "p".into();
    }
    fx.app.handle_key(key(KeyCode::Enter)).await.unwrap();
    let cs = fx.app.client_state.read().await;
    assert!(cs.server_state.status.is_some());
}

#[tokio::test]
#[serial]
async fn unhandled_key_is_silently_ignored() {
    let mut fx = build_app().await;
    fx.app.handle_key(key(KeyCode::Insert)).await.unwrap();
}

#[tokio::test]
#[serial]
async fn char_on_button_field_is_noop() {
    let mut fx = build_app().await;
    {
        let mut cs = fx.app.client_state.write().await;
        cs.server_state.selected_field = 3;
    }
    fx.app.handle_key(key(KeyCode::Char('q'))).await.unwrap();
}

#[tokio::test]
#[serial]
async fn down_advances_field_one_then_two() {
    let mut fx = build_app().await;
    {
        let mut cs = fx.app.client_state.write().await;
        cs.server_state.selected_field = 0;
    }
    fx.app.handle_key(key(KeyCode::Down)).await.unwrap();
    fx.app.handle_key(key(KeyCode::Down)).await.unwrap();
    assert_eq!(
        fx.app.client_state.read().await.server_state.selected_field,
        2
    );
}

#[tokio::test]
#[serial]
async fn up_reduces_field() {
    let mut fx = build_app().await;
    {
        let mut cs = fx.app.client_state.write().await;
        cs.server_state.selected_field = 3;
    }
    fx.app.handle_key(key(KeyCode::Up)).await.unwrap();
    assert_eq!(
        fx.app.client_state.read().await.server_state.selected_field,
        2
    );
}
