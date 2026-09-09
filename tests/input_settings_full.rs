//! Exhaustive input_settings.rs: every field, every direction, both cava states.

mod common;
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
    let tempdir = common::tempdir();
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
    for _ in 0..25 {
        fx.app.handle_key(key(KeyCode::Down)).await.unwrap();
    }
    assert_eq!(
        fx.app
            .client_state
            .read()
            .await
            .settings_state
            .selected_field,
        20
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

// Regression: h/l/space are global Previous/Next/Pause bindings everywhere
// else, but on Settings they are the page's own field-navigation keys and
// must reach handle_settings_key instead of firing a daemon request.
#[tokio::test]
#[serial]
async fn l_key_on_cover_art_size_field_advances_it_like_right() {
    let mut fx = build_app().await;
    {
        let mut cs = fx.app.client_state.write().await;
        cs.settings_state.selected_field = 4;
        cs.settings_state.cover_art_size = 10;
    }
    fx.app.handle_key(key(KeyCode::Char('l'))).await.unwrap();
    assert_eq!(
        fx.app
            .client_state
            .read()
            .await
            .settings_state
            .cover_art_size,
        12,
        "l must be routed to Settings' Right-equivalent action, not stolen as global Next"
    );
}

#[tokio::test]
#[serial]
async fn h_key_on_cover_art_size_field_reduces_it_like_left() {
    let mut fx = build_app().await;
    {
        let mut cs = fx.app.client_state.write().await;
        cs.settings_state.selected_field = 4;
        cs.settings_state.cover_art_size = 12;
    }
    fx.app.handle_key(key(KeyCode::Char('h'))).await.unwrap();
    assert_eq!(
        fx.app
            .client_state
            .read()
            .await
            .settings_state
            .cover_art_size,
        10,
        "h must be routed to Settings' Left-equivalent action, not stolen as global Previous"
    );
}

// Regression: the h/l/space carve-out for Settings must stay narrow — q
// (and other truly-global bindings) must still reach the global quit
// handler instead of being silently swallowed as an unhandled Settings key.
#[tokio::test]
#[serial]
async fn q_key_still_quits_from_settings_page() {
    let mut fx = build_app().await;
    fx.app.handle_key(key(KeyCode::Char('q'))).await.unwrap();
    assert!(fx.app.client_state.read().await.should_quit);
}

#[tokio::test]
#[serial]
async fn space_key_toggles_cover_art_on_field_three_like_enter() {
    let mut fx = build_app().await;
    {
        let mut cs = fx.app.client_state.write().await;
        cs.settings_state.selected_field = 3;
        cs.settings_state.cover_art = false;
    }
    fx.app.handle_key(key(KeyCode::Char(' '))).await.unwrap();
    assert!(
        fx.app.client_state.read().await.settings_state.cover_art,
        "space must be routed to Settings' Enter-equivalent action, not stolen as global Pause"
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
async fn daemon_enabled_field_eight_toggles() {
    let mut fx = build_app().await;
    {
        let mut cs = fx.app.client_state.write().await;
        cs.settings_state.selected_field = 8;
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
async fn scrobble_field_seven_toggles() {
    let mut fx = build_app().await;
    {
        let mut cs = fx.app.client_state.write().await;
        cs.settings_state.selected_field = 7;
        cs.settings_state.scrobble = false;
    }
    fx.app.handle_key(key(KeyCode::Right)).await.unwrap();
    assert!(fx.app.client_state.read().await.settings_state.scrobble);
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
async fn replay_gain_mode_field_ten_cycles_forward_with_right() {
    let mut fx = build_app().await;
    {
        let mut cs = fx.app.client_state.write().await;
        cs.settings_state.selected_field = 10;
        cs.settings_state.replay_gain_mode = ferrosonic::config::ReplayGainMode::Off;
    }
    fx.app.handle_key(key(KeyCode::Right)).await.unwrap();
    assert_eq!(
        fx.app
            .client_state
            .read()
            .await
            .settings_state
            .replay_gain_mode,
        ferrosonic::config::ReplayGainMode::Track
    );
}

#[tokio::test]
#[serial]
async fn replay_gain_mode_field_ten_cycles_backward_with_left() {
    let mut fx = build_app().await;
    {
        let mut cs = fx.app.client_state.write().await;
        cs.settings_state.selected_field = 10;
        cs.settings_state.replay_gain_mode = ferrosonic::config::ReplayGainMode::Off;
    }
    fx.app.handle_key(key(KeyCode::Left)).await.unwrap();
    assert_eq!(
        fx.app
            .client_state
            .read()
            .await
            .settings_state
            .replay_gain_mode,
        ferrosonic::config::ReplayGainMode::Album
    );
}

#[tokio::test]
#[serial]
async fn replay_gain_preamp_field_eleven_adjusts_by_half_db() {
    let mut fx = build_app().await;
    {
        let mut cs = fx.app.client_state.write().await;
        cs.settings_state.selected_field = 11;
        cs.settings_state.replay_gain_preamp = 0.0;
    }
    fx.app.handle_key(key(KeyCode::Right)).await.unwrap();
    assert_eq!(
        fx.app
            .client_state
            .read()
            .await
            .settings_state
            .replay_gain_preamp,
        0.5
    );
    fx.app.handle_key(key(KeyCode::Left)).await.unwrap();
    fx.app.handle_key(key(KeyCode::Left)).await.unwrap();
    assert_eq!(
        fx.app
            .client_state
            .read()
            .await
            .settings_state
            .replay_gain_preamp,
        -0.5
    );
}

#[tokio::test]
#[serial]
async fn replay_gain_preamp_field_eleven_clamps_at_bounds() {
    let mut fx = build_app().await;
    {
        let mut cs = fx.app.client_state.write().await;
        cs.settings_state.selected_field = 11;
        cs.settings_state.replay_gain_preamp = 15.0;
    }
    fx.app.handle_key(key(KeyCode::Right)).await.unwrap();
    assert_eq!(
        fx.app
            .client_state
            .read()
            .await
            .settings_state
            .replay_gain_preamp,
        15.0,
        "preamp must clamp at the mpv maximum of 15.0 dB"
    );
}

#[tokio::test]
#[serial]
async fn replay_gain_clip_field_twelve_toggles() {
    let mut fx = build_app().await;
    {
        let mut cs = fx.app.client_state.write().await;
        cs.settings_state.selected_field = 12;
        cs.settings_state.replay_gain_clip = false;
    }
    fx.app.handle_key(key(KeyCode::Right)).await.unwrap();
    assert!(
        fx.app
            .client_state
            .read()
            .await
            .settings_state
            .replay_gain_clip
    );
}

#[tokio::test]
#[serial]
async fn min_rating_field_thirteen_adjusts_and_clamps() {
    let mut fx = build_app().await;
    {
        let mut cs = fx.app.client_state.write().await;
        cs.settings_state.selected_field = 13;
        cs.settings_state.playback_filters.min_rating = 0;
    }
    fx.app.handle_key(key(KeyCode::Right)).await.unwrap();
    assert_eq!(
        fx.app
            .client_state
            .read()
            .await
            .settings_state
            .playback_filters
            .min_rating,
        1
    );
    // Hold well past the ceiling; must clamp at 5, not wrap or overflow.
    for _ in 0..10 {
        fx.app.handle_key(key(KeyCode::Right)).await.unwrap();
    }
    assert_eq!(
        fx.app
            .client_state
            .read()
            .await
            .settings_state
            .playback_filters
            .min_rating,
        5
    );
}

#[tokio::test]
#[serial]
async fn year_min_field_fourteen_cycles_off_to_a_year_and_back() {
    let mut fx = build_app().await;
    {
        let mut cs = fx.app.client_state.write().await;
        cs.settings_state.selected_field = 14;
        cs.settings_state.playback_filters.year_min = None;
    }
    fx.app.handle_key(key(KeyCode::Right)).await.unwrap();
    assert!(
        fx.app
            .client_state
            .read()
            .await
            .settings_state
            .playback_filters
            .year_min
            .is_some(),
        "Right from Off must set a concrete year"
    );
    fx.app.handle_key(key(KeyCode::Left)).await.unwrap();
    assert_eq!(
        fx.app
            .client_state
            .read()
            .await
            .settings_state
            .playback_filters
            .year_min,
        None,
        "Left back to the floor must return to Off"
    );
}

#[tokio::test]
#[serial]
async fn duration_max_field_seventeen_cycles_off_to_a_value_and_back() {
    let mut fx = build_app().await;
    {
        let mut cs = fx.app.client_state.write().await;
        cs.settings_state.selected_field = 17;
        cs.settings_state.playback_filters.duration_max_secs = None;
    }
    fx.app.handle_key(key(KeyCode::Left)).await.unwrap();
    assert!(
        fx.app
            .client_state
            .read()
            .await
            .settings_state
            .playback_filters
            .duration_max_secs
            .is_some(),
        "Left from Off must set a concrete duration (Left starts at the ceiling)"
    );
    fx.app.handle_key(key(KeyCode::Right)).await.unwrap();
    assert_eq!(
        fx.app
            .client_state
            .read()
            .await
            .settings_state
            .playback_filters
            .duration_max_secs,
        None,
        "Right back to the ceiling must return to Off"
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
