//! Library page key handlers: tree navigation, search, scope cycle.

mod common;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ferrosonic::app::state::{FilterScope, Page};
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
    let tempdir = common::tempdir();
    std::env::set_var("FERROSONIC_CONFIG_DIR", tempdir.path());
    let mut config = Config::new();
    config.daemon = false;
    let mut app = App::new(config);
    app.handle_key(key(KeyCode::F(1))).await.unwrap();
    AppFixture {
        app,
        _tempdir: tempdir,
    }
}

#[tokio::test]
#[serial]
async fn slash_opens_search_then_types_into_filter() {
    let mut fx = build_app().await;
    fx.app.handle_key(key(KeyCode::Char('/'))).await.unwrap();
    fx.app.handle_key(key(KeyCode::Char('a'))).await.unwrap();
    fx.app.handle_key(key(KeyCode::Char('b'))).await.unwrap();
    let cs = fx.app.client_state.read().await;
    assert!(cs.artists.filter_active);
    assert_eq!(cs.artists.filter, "ab");
}

#[tokio::test]
#[serial]
async fn backspace_removes_last_filter_char() {
    let mut fx = build_app().await;
    fx.app.handle_key(key(KeyCode::Char('/'))).await.unwrap();
    fx.app.handle_key(key(KeyCode::Char('x'))).await.unwrap();
    fx.app.handle_key(key(KeyCode::Char('y'))).await.unwrap();
    fx.app.handle_key(key(KeyCode::Backspace)).await.unwrap();
    let cs = fx.app.client_state.read().await;
    assert_eq!(cs.artists.filter, "x");
}

#[tokio::test]
#[serial]
async fn esc_closes_and_clears_filter() {
    let mut fx = build_app().await;
    fx.app.handle_key(key(KeyCode::Char('/'))).await.unwrap();
    fx.app.handle_key(key(KeyCode::Char('h'))).await.unwrap();
    fx.app.handle_key(key(KeyCode::Esc)).await.unwrap();
    let cs = fx.app.client_state.read().await;
    assert!(!cs.artists.filter_active);
    assert!(cs.artists.filter.is_empty());
}

#[tokio::test]
#[serial]
async fn enter_closes_filter_but_keeps_content() {
    let mut fx = build_app().await;
    fx.app.handle_key(key(KeyCode::Char('/'))).await.unwrap();
    fx.app.handle_key(key(KeyCode::Char('q'))).await.unwrap();
    fx.app.handle_key(key(KeyCode::Enter)).await.unwrap();
    let cs = fx.app.client_state.read().await;
    assert!(!cs.artists.filter_active);
    assert_eq!(cs.artists.filter, "q");
}

#[tokio::test]
#[serial]
async fn slash_on_empty_filter_cycles_scope() {
    let mut fx = build_app().await;
    fx.app.handle_key(key(KeyCode::Char('/'))).await.unwrap();
    assert_eq!(
        fx.app.client_state.read().await.artists.filter_scope,
        FilterScope::Artists
    );

    fx.app.handle_key(key(KeyCode::Char('/'))).await.unwrap();
    assert_eq!(
        fx.app.client_state.read().await.artists.filter_scope,
        FilterScope::Albums
    );

    fx.app.handle_key(key(KeyCode::Char('/'))).await.unwrap();
    assert_eq!(
        fx.app.client_state.read().await.artists.filter_scope,
        FilterScope::Songs
    );

    fx.app.handle_key(key(KeyCode::Char('/'))).await.unwrap();
    assert_eq!(
        fx.app.client_state.read().await.artists.filter_scope,
        FilterScope::Artists
    );
}

#[tokio::test]
#[serial]
async fn slash_on_non_empty_filter_appends_literal_slash() {
    let mut fx = build_app().await;
    fx.app.handle_key(key(KeyCode::Char('/'))).await.unwrap();
    fx.app.handle_key(key(KeyCode::Char('x'))).await.unwrap();
    fx.app.handle_key(key(KeyCode::Char('/'))).await.unwrap();
    let cs = fx.app.client_state.read().await;
    assert_eq!(cs.artists.filter, "x/", "// in non-empty filter is literal");
}

#[tokio::test]
#[serial]
async fn library_page_is_active_after_f1() {
    let fx = build_app().await;
    let cs = fx.app.client_state.read().await;
    assert_eq!(cs.page, Page::Library);
}
