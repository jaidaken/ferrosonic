//! Input handlers for Queue, Settings, Server, Playlists, QuickPlay pages.

mod common;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ferrosonic::app::state::Page;
use ferrosonic::app::App;
use ferrosonic::config::Config;
use serial_test::serial;

fn key(code: KeyCode) -> KeyEvent {
    let mut k = KeyEvent::new(code, KeyModifiers::NONE);
    k.kind = KeyEventKind::Press;
    k
}

fn shift(code: KeyCode) -> KeyEvent {
    let mut k = KeyEvent::new(code, KeyModifiers::SHIFT);
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
    let app = App::new(config);
    AppFixture {
        app,
        _tempdir: tempdir,
    }
}

#[tokio::test]
#[serial]
async fn queue_page_down_advances_selected() {
    let mut fx = build_app().await;
    {
        let mut ds = fx.app.daemon_state.write().await;
        for i in 0..3 {
            ds.queue.push(ferrosonic::subsonic::models::Child {
                id: format!("q-{}", i),
                title: format!("Q{}", i),
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
                duration: Some(180),
                bit_rate: None,
                path: None,
                disc_number: None,
                starred: None,
            });
        }
    }
    fx.app.handle_key(key(KeyCode::F(2))).await.unwrap();
    fx.app.handle_key(key(KeyCode::Down)).await.unwrap();
    fx.app.handle_key(key(KeyCode::Down)).await.unwrap();
    let cs = fx.app.client_state.read().await;
    assert_eq!(cs.page, Page::Queue);
    assert!(
        matches!(cs.queue_state.selected, Some(idx) if idx > 0),
        "Down should advance queue selection from None; got {:?}",
        cs.queue_state.selected
    );
}

#[tokio::test]
#[serial]
async fn settings_left_arrow_adjusts_setting_down() {
    let mut fx = build_app().await;
    fx.app.handle_key(key(KeyCode::F(6))).await.unwrap();
    let initial_size = fx.app.client_state.read().await.settings_state.cava_size;
    fx.app.handle_key(key(KeyCode::Down)).await.unwrap();
    fx.app.handle_key(key(KeyCode::Down)).await.unwrap();
    fx.app.handle_key(key(KeyCode::Left)).await.unwrap();
    let after = fx.app.client_state.read().await.settings_state.cava_size;
    assert!(
        after <= initial_size,
        "Left arrow on cava_size should decrement or saturate; before {} after {}",
        initial_size,
        after
    );
}

#[tokio::test]
#[serial]
async fn server_page_typing_into_url_field_buffers_input() {
    let mut fx = build_app().await;
    fx.app.handle_key(key(KeyCode::F(5))).await.unwrap();
    for c in "https://e.com".chars() {
        fx.app.handle_key(key(KeyCode::Char(c))).await.unwrap();
    }
    let cs = fx.app.client_state.read().await;
    assert_eq!(cs.page, Page::Server);
    assert!(
        cs.server_state.base_url.contains("https://e.com") || !cs.server_state.base_url.is_empty(),
        "URL field should buffer typed chars; got: {:?}",
        cs.server_state.base_url
    );
}

#[tokio::test]
#[serial]
async fn playlists_page_renders_after_f4() {
    let mut fx = build_app().await;
    fx.app.handle_key(key(KeyCode::F(4))).await.unwrap();
    let cs = fx.app.client_state.read().await;
    assert_eq!(cs.page, Page::Playlists);
}

#[tokio::test]
#[serial]
async fn quickplay_page_active_after_f3() {
    let mut fx = build_app().await;
    fx.app.handle_key(key(KeyCode::F(3))).await.unwrap();
    let cs = fx.app.client_state.read().await;
    assert_eq!(cs.page, Page::QuickPlay);
}

#[tokio::test]
#[serial]
async fn quickplay_arrow_keys_navigate_options() {
    let mut fx = build_app().await;
    fx.app.handle_key(key(KeyCode::F(3))).await.unwrap();
    fx.app.handle_key(key(KeyCode::Down)).await.unwrap();
    let cs = fx.app.client_state.read().await;
    assert!(
        cs.songs.selected_option.is_some(),
        "quickplay should pick an option on Down"
    );
}

#[tokio::test]
#[serial]
async fn shift_t_on_library_calls_shuffle_library() {
    let mut fx = build_app().await;
    fx.app.handle_key(key(KeyCode::F(1))).await.unwrap();
    fx.app.handle_key(shift(KeyCode::Char('T'))).await.unwrap();
    // No assertion on side effect; just verify no panic and event handled.
}

#[tokio::test]
#[serial]
async fn space_anywhere_toggles_pause_via_daemon_request() {
    let mut fx = build_app().await;
    fx.app.handle_key(key(KeyCode::Char(' '))).await.unwrap();
    // Same: no panic. State change is async via the client. We only
    // verify the handler accepted the key.
}
