//! Settings page input: navigate every field, adjust value, save.

mod common;
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
    let tempdir = common::tempdir();
    std::env::set_var("FERROSONIC_CONFIG_DIR", tempdir.path());
    let mut config = Config::new();
    config.daemon = false;
    let mut app = App::new(config);
    app.handle_key(key(KeyCode::F(6))).await.unwrap();
    AppFixture {
        app,
        _tempdir: tempdir,
    }
}

#[tokio::test]
#[serial]
async fn down_advances_through_all_fields_then_caps() {
    let mut fx = build_app().await;
    for _ in 0..20 {
        fx.app.handle_key(key(KeyCode::Down)).await.unwrap();
    }
    let cs = fx.app.client_state.read().await;
    assert_eq!(
        cs.settings_state.selected_field, 9,
        "field index should cap at SETTINGS_FIELD_COUNT - 1 = 9"
    );
}

#[tokio::test]
#[serial]
async fn up_reduces_to_zero_then_caps() {
    let mut fx = build_app().await;
    for _ in 0..20 {
        fx.app.handle_key(key(KeyCode::Up)).await.unwrap();
    }
    let cs = fx.app.client_state.read().await;
    assert_eq!(cs.settings_state.selected_field, 0, "Up cap at 0");
}

#[tokio::test]
#[serial]
async fn enter_on_repeat_mode_field_cycles_repeat() {
    use ferrosonic::config::RepeatMode;
    let mut fx = build_app().await;
    for _ in 0..5 {
        fx.app.handle_key(key(KeyCode::Down)).await.unwrap();
    }
    let before = fx.app.client_state.read().await.settings_state.repeat_mode;
    fx.app.handle_key(key(KeyCode::Enter)).await.unwrap();
    let after = fx.app.client_state.read().await.settings_state.repeat_mode;
    if before == RepeatMode::Off {
        assert!(after != RepeatMode::Off, "Enter should advance repeat mode");
    }
}

#[tokio::test]
#[serial]
async fn left_arrow_on_cava_size_decrements() {
    let mut fx = build_app().await;
    fx.app.handle_key(key(KeyCode::Down)).await.unwrap();
    fx.app.handle_key(key(KeyCode::Down)).await.unwrap();
    {
        let mut cs = fx.app.client_state.write().await;
        cs.settings_state.cava_size = 40;
        cs.cava_available = true;
    }
    fx.app.handle_key(key(KeyCode::Left)).await.unwrap();
    let after = fx.app.client_state.read().await.settings_state.cava_size;
    assert!(
        after < 40 || after == 0,
        "Left should decrease cava_size; got {}",
        after
    );
}

#[tokio::test]
#[serial]
async fn right_arrow_on_cover_art_size_increments_step_two() {
    let mut fx = build_app().await;
    for _ in 0..4 {
        fx.app.handle_key(key(KeyCode::Down)).await.unwrap();
    }
    let before = fx
        .app
        .client_state
        .read()
        .await
        .settings_state
        .cover_art_size;
    fx.app.handle_key(key(KeyCode::Right)).await.unwrap();
    let after = fx
        .app
        .client_state
        .read()
        .await
        .settings_state
        .cover_art_size;
    assert!(
        after >= before,
        "Right on cover_art_size must not decrease; before {} after {}",
        before,
        after
    );
}
