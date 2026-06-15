//! App key-event dispatch across pages.

mod common;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ferrosonic::app::state::Page;
use ferrosonic::app::App;
use ferrosonic::config::Config;
use serial_test::serial;

fn key_with_kind(code: KeyCode) -> KeyEvent {
    let mut k = KeyEvent::new(code, KeyModifiers::NONE);
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
async fn f1_through_f6_switch_pages() {
    let mut app = build_app().await;
    let cases = [
        (KeyCode::F(1), Page::Library),
        (KeyCode::F(2), Page::Queue),
        (KeyCode::F(3), Page::QuickPlay),
        (KeyCode::F(4), Page::Playlists),
        (KeyCode::F(5), Page::Server),
        (KeyCode::F(6), Page::Settings),
    ];
    for (code, expected) in cases {
        app.handle_key(key_with_kind(code))
            .await
            .expect("handle_key f-key");
        let cs = app.client_state.read().await;
        assert_eq!(
            cs.page, expected,
            "F-key {:?} should switch to {:?}",
            code, expected
        );
    }
}

#[tokio::test]
#[serial]
async fn q_at_top_level_quits() {
    let mut app = build_app().await;
    app.handle_key(key_with_kind(KeyCode::Char('q')))
        .await
        .unwrap();
    let cs = app.client_state.read().await;
    assert!(cs.should_quit, "q must set should_quit");
}

#[tokio::test]
#[serial]
async fn r_cycles_repeat_mode() {
    use ferrosonic::config::RepeatMode;

    let mut app = build_app().await;
    let r = || key_with_kind(KeyCode::Char('r'));
    assert_eq!(
        app.daemon_state.read().await.config.repeat_mode,
        RepeatMode::Off
    );

    app.handle_key(r()).await.unwrap();
    assert_eq!(
        app.client_state.read().await.settings_state.repeat_mode,
        RepeatMode::One,
        "Off -> One"
    );

    app.handle_key(r()).await.unwrap();
    assert_eq!(
        app.client_state.read().await.settings_state.repeat_mode,
        RepeatMode::All,
        "One -> All"
    );

    app.handle_key(r()).await.unwrap();
    assert_eq!(
        app.client_state.read().await.settings_state.repeat_mode,
        RepeatMode::Off,
        "All -> Off"
    );
}

#[tokio::test]
#[serial]
async fn arrow_keys_navigate_settings_fields() {
    let mut app = build_app().await;
    app.handle_key(key_with_kind(KeyCode::F(6))).await.unwrap();
    assert_eq!(app.client_state.read().await.page, Page::Settings);

    let initial = app.client_state.read().await.settings_state.selected_field;
    app.handle_key(key_with_kind(KeyCode::Down)).await.unwrap();
    let after_down = app.client_state.read().await.settings_state.selected_field;
    assert_eq!(after_down, initial + 1, "Down should advance field index");

    app.handle_key(key_with_kind(KeyCode::Up)).await.unwrap();
    let after_up = app.client_state.read().await.settings_state.selected_field;
    assert_eq!(after_up, initial, "Up should reverse to initial field");
}

#[tokio::test]
#[serial]
async fn key_release_is_ignored() {
    let mut app = build_app().await;
    let mut k = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE);
    k.kind = KeyEventKind::Release;
    use crossterm::event::Event;
    app.handle_event(Event::Key(k)).await.unwrap();
    let cs = app.client_state.read().await;
    assert!(!cs.should_quit, "release event must not quit");
}

#[tokio::test]
#[serial]
async fn slash_opens_search_on_library_page() {
    let mut app = build_app().await;
    app.handle_key(key_with_kind(KeyCode::F(1))).await.unwrap();
    app.handle_key(key_with_kind(KeyCode::Char('/')))
        .await
        .unwrap();
    let cs = app.client_state.read().await;
    assert!(
        cs.artists.filter_active,
        "slash should open the search bar (filter_active=true)"
    );
}
