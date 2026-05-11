//! Exhaustive input_settings.rs: every field, every direction, both cava states.

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ferrosonic::app::App;
use ferrosonic::config::{Config, RepeatMode};
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
    app.handle_key(key(KeyCode::F(6))).await.unwrap();
    {
        let mut cs = app.client_state.write().await;
        cs.cava_available = true;
        cs.settings_state.selected_field = 0;
    }
    AppFixture {
        app,
        _tempdir: tempdir,
    }
}

#[tokio::test]
#[serial]
async fn up_stays_at_zero_field() {
    let mut fx = build_app().await;
    fx.app.handle_key(key(KeyCode::Up)).await.unwrap();
    assert_eq!(
        fx.app
            .client_state
            .read()
            .await
            .settings_state
            .selected_field,
        0
    );
}

#[tokio::test]
#[serial]
async fn down_stops_at_max_field() {
    let mut fx = build_app().await;
    for _ in 0..15 {
        fx.app.handle_key(key(KeyCode::Down)).await.unwrap();
    }
    assert_eq!(
        fx.app
            .client_state
            .read()
            .await
            .settings_state
            .selected_field,
        7
    );
}

#[tokio::test]
#[serial]
async fn k_acts_as_up() {
    let mut fx = build_app().await;
    fx.app.handle_key(key(KeyCode::Down)).await.unwrap();
    fx.app.handle_key(key(KeyCode::Char('k'))).await.unwrap();
    assert_eq!(
        fx.app
            .client_state
            .read()
            .await
            .settings_state
            .selected_field,
        0
    );
}

#[tokio::test]
#[serial]
async fn j_acts_as_down() {
    let mut fx = build_app().await;
    fx.app.handle_key(key(KeyCode::Char('j'))).await.unwrap();
    assert_eq!(
        fx.app
            .client_state
            .read()
            .await
            .settings_state
            .selected_field,
        1
    );
}

#[tokio::test]
#[serial]
async fn enter_toggles_cover_art_on_field_three() {
    let mut fx = build_app().await;
    {
        let mut cs = fx.app.client_state.write().await;
        cs.settings_state.selected_field = 3;
        cs.settings_state.cover_art = false;
    }
    fx.app.handle_key(key(KeyCode::Enter)).await.unwrap();
    assert!(fx.app.client_state.read().await.settings_state.cover_art);
}

#[tokio::test]
#[serial]
async fn enter_advances_cover_art_size() {
    let mut fx = build_app().await;
    {
        let mut cs = fx.app.client_state.write().await;
        cs.settings_state.selected_field = 4;
        cs.settings_state.cover_art_size = 10;
    }
    fx.app.handle_key(key(KeyCode::Enter)).await.unwrap();
    assert_eq!(
        fx.app
            .client_state
            .read()
            .await
            .settings_state
            .cover_art_size,
        12
    );
}

#[tokio::test]
#[serial]
async fn left_arrow_reduces_cover_art_size() {
    let mut fx = build_app().await;
    {
        let mut cs = fx.app.client_state.write().await;
        cs.settings_state.selected_field = 4;
        cs.settings_state.cover_art_size = 12;
    }
    fx.app.handle_key(key(KeyCode::Left)).await.unwrap();
    assert_eq!(
        fx.app
            .client_state
            .read()
            .await
            .settings_state
            .cover_art_size,
        10
    );
}

#[tokio::test]
#[serial]
async fn left_at_min_cover_art_size_is_clamped_noop() {
    let mut fx = build_app().await;
    {
        let mut cs = fx.app.client_state.write().await;
        cs.settings_state.selected_field = 4;
        cs.settings_state.cover_art_size = 8;
    }
    fx.app.handle_key(key(KeyCode::Left)).await.unwrap();
    assert_eq!(
        fx.app
            .client_state
            .read()
            .await
            .settings_state
            .cover_art_size,
        8
    );
}

#[tokio::test]
#[serial]
async fn right_at_max_cover_art_size_is_clamped() {
    let mut fx = build_app().await;
    {
        let mut cs = fx.app.client_state.write().await;
        cs.settings_state.selected_field = 4;
        cs.settings_state.cover_art_size = 24;
    }
    fx.app.handle_key(key(KeyCode::Right)).await.unwrap();
    assert_eq!(
        fx.app
            .client_state
            .read()
            .await
            .settings_state
            .cover_art_size,
        24
    );
}

#[tokio::test]
#[serial]
async fn repeat_field_cycles_forward_with_right() {
    let mut fx = build_app().await;
    {
        let mut cs = fx.app.client_state.write().await;
        cs.settings_state.selected_field = 5;
        cs.settings_state.repeat_mode = RepeatMode::Off;
    }
    fx.app.handle_key(key(KeyCode::Right)).await.unwrap();
    assert_eq!(
        fx.app.client_state.read().await.settings_state.repeat_mode,
        RepeatMode::One
    );
}

#[tokio::test]
#[serial]
async fn repeat_field_cycles_backward_with_left() {
    let mut fx = build_app().await;
    {
        let mut cs = fx.app.client_state.write().await;
        cs.settings_state.selected_field = 5;
        cs.settings_state.repeat_mode = RepeatMode::Off;
    }
    fx.app.handle_key(key(KeyCode::Left)).await.unwrap();
    assert_eq!(
        fx.app.client_state.read().await.settings_state.repeat_mode,
        RepeatMode::All
    );
}

#[tokio::test]
#[serial]
async fn repeat_field_left_from_one_goes_to_off() {
    let mut fx = build_app().await;
    {
        let mut cs = fx.app.client_state.write().await;
        cs.settings_state.selected_field = 5;
        cs.settings_state.repeat_mode = RepeatMode::One;
    }
    fx.app.handle_key(key(KeyCode::Left)).await.unwrap();
    assert_eq!(
        fx.app.client_state.read().await.settings_state.repeat_mode,
        RepeatMode::Off
    );
}

#[tokio::test]
#[serial]
async fn repeat_field_left_from_all_goes_to_one() {
    let mut fx = build_app().await;
    {
        let mut cs = fx.app.client_state.write().await;
        cs.settings_state.selected_field = 5;
        cs.settings_state.repeat_mode = RepeatMode::All;
    }
    fx.app.handle_key(key(KeyCode::Left)).await.unwrap();
    assert_eq!(
        fx.app.client_state.read().await.settings_state.repeat_mode,
        RepeatMode::One
    );
}

#[tokio::test]
#[serial]
async fn auto_continue_field_six_toggles() {
    let mut fx = build_app().await;
    {
        let mut cs = fx.app.client_state.write().await;
        cs.settings_state.selected_field = 6;
        cs.settings_state.auto_continue = false;
    }
    fx.app.handle_key(key(KeyCode::Right)).await.unwrap();
    assert!(
        fx.app
            .client_state
            .read()
            .await
            .settings_state
            .auto_continue
    );
}

#[tokio::test]
#[serial]
async fn daemon_enabled_field_seven_toggles() {
    let mut fx = build_app().await;
    {
        let mut cs = fx.app.client_state.write().await;
        cs.settings_state.selected_field = 7;
        cs.settings_state.daemon_enabled = false;
    }
    fx.app.handle_key(key(KeyCode::Right)).await.unwrap();
    assert!(
        fx.app
            .client_state
            .read()
            .await
            .settings_state
            .daemon_enabled
    );
}

#[tokio::test]
#[serial]
async fn cava_field_one_toggles_when_cava_available() {
    let mut fx = build_app().await;
    {
        let mut cs = fx.app.client_state.write().await;
        cs.cava_available = true;
        cs.settings_state.selected_field = 1;
        cs.settings_state.cava_enabled = false;
    }
    fx.app.handle_key(key(KeyCode::Right)).await.unwrap();
    assert!(fx.app.client_state.read().await.settings_state.cava_enabled);
}

#[tokio::test]
#[serial]
async fn cava_field_one_is_noop_when_cava_unavailable() {
    let mut fx = build_app().await;
    {
        let mut cs = fx.app.client_state.write().await;
        cs.cava_available = false;
        cs.settings_state.selected_field = 1;
        cs.settings_state.cava_enabled = false;
    }
    fx.app.handle_key(key(KeyCode::Right)).await.unwrap();
    assert!(!fx.app.client_state.read().await.settings_state.cava_enabled);
}

#[tokio::test]
#[serial]
async fn cava_size_field_two_adjusts_when_available() {
    let mut fx = build_app().await;
    {
        let mut cs = fx.app.client_state.write().await;
        cs.cava_available = true;
        cs.settings_state.selected_field = 2;
        cs.settings_state.cava_size = 20;
    }
    fx.app.handle_key(key(KeyCode::Right)).await.unwrap();
    assert_eq!(
        fx.app.client_state.read().await.settings_state.cava_size,
        25
    );
}

#[tokio::test]
#[serial]
async fn cava_size_clamps_at_minimum() {
    let mut fx = build_app().await;
    {
        let mut cs = fx.app.client_state.write().await;
        cs.cava_available = true;
        cs.settings_state.selected_field = 2;
        cs.settings_state.cava_size = 10;
    }
    fx.app.handle_key(key(KeyCode::Left)).await.unwrap();
    assert_eq!(
        fx.app.client_state.read().await.settings_state.cava_size,
        10
    );
}

#[tokio::test]
#[serial]
async fn cava_size_clamps_at_max() {
    let mut fx = build_app().await;
    {
        let mut cs = fx.app.client_state.write().await;
        cs.cava_available = true;
        cs.settings_state.selected_field = 2;
        cs.settings_state.cava_size = 80;
    }
    fx.app.handle_key(key(KeyCode::Right)).await.unwrap();
    assert_eq!(
        fx.app.client_state.read().await.settings_state.cava_size,
        80
    );
}

#[tokio::test]
#[serial]
async fn unhandled_key_is_noop() {
    let mut fx = build_app().await;
    fx.app.handle_key(key(KeyCode::Insert)).await.unwrap();
}

#[tokio::test]
#[serial]
async fn down_then_up_reverses() {
    let mut fx = build_app().await;
    fx.app.handle_key(key(KeyCode::Down)).await.unwrap();
    fx.app.handle_key(key(KeyCode::Down)).await.unwrap();
    fx.app.handle_key(key(KeyCode::Up)).await.unwrap();
    assert_eq!(
        fx.app
            .client_state
            .read()
            .await
            .settings_state
            .selected_field,
        1
    );
}
