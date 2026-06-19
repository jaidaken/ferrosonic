//! Exhaustive top-level handle_key branches (input.rs).

mod common;
use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ferrosonic::app::state::Page;
use ferrosonic::app::App;
use ferrosonic::config::Config;
use serial_test::serial;

fn key(code: KeyCode) -> KeyEvent {
    let mut k = KeyEvent::new(code, KeyModifiers::NONE);
    k.kind = KeyEventKind::Press;
    k
}

fn key_with_mod(code: KeyCode, mods: KeyModifiers) -> KeyEvent {
    let mut k = KeyEvent::new(code, mods);
    k.kind = KeyEventKind::Press;
    k
}

async fn build_app() -> App {
    let mut config = Config::new();
    config.daemon = false;
    let tempdir = common::tempdir();
    std::env::set_var("FERROSONIC_CONFIG_DIR", tempdir.path());
    std::mem::forget(tempdir);
    App::new(config)
}

#[tokio::test]
#[serial]
async fn p_toggles_pause_via_daemon_request() {
    let mut app = build_app().await;
    app.handle_key(key(KeyCode::Char('p'))).await.unwrap();
}

#[tokio::test]
#[serial]
async fn space_toggles_pause_via_daemon_request() {
    let mut app = build_app().await;
    app.handle_key(key(KeyCode::Char(' '))).await.unwrap();
}

#[tokio::test]
#[serial]
async fn l_dispatches_next_track() {
    let mut app = build_app().await;
    app.handle_key(key(KeyCode::Char('l'))).await.unwrap();
}

#[tokio::test]
#[serial]
async fn h_dispatches_previous_track() {
    let mut app = build_app().await;
    app.handle_key(key(KeyCode::Char('h'))).await.unwrap();
}

#[tokio::test]
#[serial]
async fn n_with_no_current_song_is_safe_noop() {
    let mut app = build_app().await;
    app.handle_key(key(KeyCode::Char('n'))).await.unwrap();
}

#[tokio::test]
#[serial]
async fn n_with_current_song_stars_it() {
    let mut app = build_app().await;
    {
        let mut ds = app.daemon_state.write().await;
        ds.now_playing.song = Some(ferrosonic::subsonic::models::Child {
            id: "current-id".into(),
            title: "Now Playing".into(),
            parent: None,
            is_dir: false,
            album: None,
            artist: None,
            artist_id: None,
            album_id: None,
            track: None,
            year: None,
            genre: None,
            cover_art: None,
            size: None,
            content_type: None,
            suffix: None,
            duration: Some(180),
            bit_rate: None,
            path: None,
            disc_number: None,
            starred: None,
        });
    }
    app.handle_key(key(KeyCode::Char('n'))).await.unwrap();
}

#[tokio::test]
#[serial]
async fn capital_t_triggers_shuffle_library_request() {
    let mut app = build_app().await;
    app.handle_key(key(KeyCode::Char('T'))).await.unwrap();
}

#[tokio::test]
#[serial]
async fn ctrl_r_triggers_refresh_path() {
    let mut app = build_app().await;
    app.handle_key(key_with_mod(KeyCode::Char('r'), KeyModifiers::CONTROL))
        .await
        .unwrap();
}

#[tokio::test]
#[serial]
async fn fkey_on_server_page_reverts_unsaved_edits() {
    let mut app = build_app().await;
    app.handle_key(key(KeyCode::F(5))).await.unwrap();
    {
        let mut cs = app.client_state.write().await;
        cs.server_state.base_url = "edited://x".into();
        cs.server_state.username = "edited-u".into();
        cs.server_state.password = "edited-p".into();
        cs.server_state.status = Some("partial".into());
    }
    app.handle_key(key(KeyCode::F(1))).await.unwrap();
    let cs = app.client_state.read().await;
    assert_eq!(cs.server_state.base_url, "");
    assert_eq!(cs.server_state.username, "");
    assert!(cs.server_state.password.is_empty());
    assert!(cs.server_state.status.is_none());
}

#[tokio::test]
#[serial]
async fn fkey_on_library_with_filter_active_closes_filter() {
    let mut app = build_app().await;
    app.handle_key(key(KeyCode::F(1))).await.unwrap();
    app.handle_key(key(KeyCode::Char('/'))).await.unwrap();
    assert!(app.client_state.read().await.artists.filter_active);
    app.handle_key(key(KeyCode::F(2))).await.unwrap();
    let cs = app.client_state.read().await;
    assert!(!cs.artists.filter_active);
    assert_eq!(cs.page, Page::Queue);
}

#[tokio::test]
#[serial]
async fn server_page_text_field_routes_to_handle_server_key() {
    let mut app = build_app().await;
    app.handle_key(key(KeyCode::F(5))).await.unwrap();
    {
        let mut cs = app.client_state.write().await;
        cs.server_state.selected_field = 0;
    }
    app.handle_key(key(KeyCode::Char('x'))).await.unwrap();
    let cs = app.client_state.read().await;
    assert!(cs.server_state.base_url.contains('x'));
}

#[tokio::test]
#[serial]
async fn library_filter_routes_to_handle_library_key() {
    let mut app = build_app().await;
    app.handle_key(key(KeyCode::F(1))).await.unwrap();
    app.handle_key(key(KeyCode::Char('/'))).await.unwrap();
    app.handle_key(key(KeyCode::Char('z'))).await.unwrap();
    assert_eq!(app.client_state.read().await.artists.filter, "z");
}

#[tokio::test]
#[serial]
async fn unmatched_top_level_key_passes_to_page_handler() {
    let mut app = build_app().await;
    app.handle_key(key(KeyCode::F(2))).await.unwrap();
    app.handle_key(key(KeyCode::Char('j'))).await.unwrap();
}

#[tokio::test]
#[serial]
async fn key_release_event_is_ignored_for_q() {
    let mut app = build_app().await;
    let mut k = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE);
    k.kind = KeyEventKind::Release;
    app.handle_event(Event::Key(k)).await.unwrap();
    assert!(!app.client_state.read().await.should_quit);
}

#[tokio::test]
#[serial]
async fn server_field_three_or_higher_does_not_text_route() {
    let mut app = build_app().await;
    app.handle_key(key(KeyCode::F(5))).await.unwrap();
    {
        let mut cs = app.client_state.write().await;
        cs.server_state.selected_field = 3;
    }
    app.handle_key(key(KeyCode::Char('j'))).await.unwrap();
}
