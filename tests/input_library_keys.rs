//! Library page key handlers: tree navigation and unified search.

mod common;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ferrosonic::app::state::Page;
use ferrosonic::app::App;
use ferrosonic::config::Config;
use ferrosonic::subsonic::models::{Album, Artist, SearchResult3};
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
async fn down_lands_on_the_greyed_album_artist_now_selectable() {
    let mut fx = build_app().await;
    {
        let mut cs = fx.app.client_state.write().await;
        cs.artists.focus = 0;
        cs.artists.filter = "a".into();
        cs.artists.filter_active = false;
        cs.artists.search_results = Some(SearchResult3 {
            artist: vec![Artist {
                id: "a1".into(),
                name: "Matched Artist".into(),
                album_count: Some(1),
                cover_art: None,
            }],
            album: vec![Album {
                id: "alb1".into(),
                name: "An Album".into(),
                artist: Some("Other Artist".into()),
                artist_id: Some("a2".into()),
                cover_art: None,
                song_count: Some(1),
                original_release_date: None,
                duration: Some(100),
                year: Some(2000),
                genre: None,
            }],
            song: vec![],
        });
        cs.artists.selected_index = Some(0);
    }

    // tree = [Artist(0), greyed Artist(1), Album(2)]. The greyed album-artist
    // carries an id now, so it is selectable: Down lands on it, not the album.
    fx.app.handle_key(key(KeyCode::Down)).await.unwrap();

    assert_eq!(
        fx.app.client_state.read().await.artists.selected_index,
        Some(1),
        "Down lands on the greyed album-artist, which is now selectable"
    );
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
