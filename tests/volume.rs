//! In-app volume: `-`/`+` (numpad or top row, `=` aliases `+`) step 1 %,
//! `[`/`]` step 5 %. The daemon clamps to 0-100, applies it to mpv, persists
//! it as `Volume` in the config and broadcasts the lightweight
//! `VolumeChanged` event (never `NowPlayingChanged`, which fans out to
//! MPRIS). The TUI shows `♪ NN%` permanently in the quality row and, for
//! two seconds after a change, a fine-grained slider in place of the
//! progress bar.

mod common;

use std::sync::Arc;
use std::time::{Duration, Instant};

use common::{fixtures::song, RecordingClient, TestDaemon};
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ferrosonic::app::App;
use ferrosonic::config::Config;
use ferrosonic::daemon::state::PlaybackState;
use ferrosonic::ipc::protocol::{DaemonEvent, DaemonRequest};
use ferrosonic::ui::widget_now_playing::volume_bar;
use serde_json::json;
use serial_test::serial;

fn press(code: KeyCode, mods: KeyModifiers) -> KeyEvent {
    let mut k = KeyEvent::new(code, mods);
    k.kind = KeyEventKind::Press;
    k
}

struct RemoteApp {
    app: App,
    rec: Arc<RecordingClient>,
    _tempdir: tempfile::TempDir,
}

async fn build_remote_app(volume: u8) -> RemoteApp {
    let tempdir = common::tempdir();
    std::env::set_var("FERROSONIC_CONFIG_DIR", tempdir.path());
    let rec = RecordingClient::new();
    let mut config = Config::new();
    config.daemon = true;
    config.volume = volume;
    let client: Arc<dyn ferrosonic::ipc::client::DaemonClient> = rec.clone();
    let app = App::with_remote_client(client, config);
    RemoteApp {
        app,
        rec,
        _tempdir: tempdir,
    }
}

// ---------------------------------------------------------------- config

#[test]
#[serial]
fn volume_defaults_to_100_and_round_trips_through_the_config_file() {
    let cfg = Config::new();
    assert_eq!(cfg.volume, 100, "default volume is full scale");

    let parsed: Config = toml::from_str("Volume = 72\n").expect("parse Volume");
    assert_eq!(parsed.volume, 72);

    let dir = common::tempdir();
    std::env::set_var("FERROSONIC_CONFIG_DIR", dir.path());
    parsed.save_default().expect("save config");
    let written = std::fs::read_to_string(dir.path().join("config.toml")).expect("read back");
    assert!(
        written.contains("Volume = 72"),
        "Volume is persisted: {written}"
    );
}

// ---------------------------------------------------------------- daemon

#[tokio::test]
#[serial]
async fn set_volume_applies_to_mpv_persists_and_emits_volume_changed() {
    let td = TestDaemon::new().await;
    let mut rx = td.core.subscribe();

    td.core.set_volume(72).await.expect("set_volume");

    let cmds = td.fake_mpv.commands().await;
    assert!(
        cmds.iter()
            .any(|c| c == &vec![json!("set_property"), json!("volume"), json!(72)]),
        "mpv receives set_property volume 72: {cmds:?}"
    );
    assert_eq!(td.state.read().await.config.volume, 72);

    let on_disk =
        std::fs::read_to_string(td.config_dir.path().join("config.toml")).expect("config written");
    assert!(on_disk.contains("Volume = 72"), "persisted: {on_disk}");

    let mut saw = None;
    while let Ok(ev) = rx.try_recv() {
        match ev {
            DaemonEvent::VolumeChanged(v) => saw = Some(v),
            DaemonEvent::NowPlayingChanged(_) => {
                panic!("volume must not fan out through NowPlayingChanged (MPRIS spam)")
            }
            _ => {}
        }
    }
    assert_eq!(saw, Some(72), "a VolumeChanged event is broadcast");
}

#[tokio::test]
#[serial]
async fn set_volume_clamps_to_0_100() {
    let td = TestDaemon::new().await;

    td.core.set_volume(150).await.expect("set_volume high");
    assert_eq!(td.state.read().await.config.volume, 100);

    td.core.set_volume(-20).await.expect("set_volume low");
    assert_eq!(td.state.read().await.config.volume, 0);

    let cmds = td.fake_mpv.commands().await;
    assert!(
        cmds.iter()
            .any(|c| c == &vec![json!("set_property"), json!("volume"), json!(100)]),
        "clamped high value reaches mpv: {cmds:?}"
    );
    assert!(
        cmds.iter()
            .any(|c| c == &vec![json!("set_property"), json!("volume"), json!(0)]),
        "clamped low value reaches mpv: {cmds:?}"
    );
}

#[tokio::test]
#[serial]
async fn daemon_restores_the_configured_volume_into_mpv_on_start() {
    // The harness builds its config in code, so seed the loaded value the
    // way `run()` sees it after reading `Volume = 37` from disk.
    let td = TestDaemon::new().await;
    td.state.write().await.config.volume = 37;

    td.core.restore_volume().await;

    let cmds = td.fake_mpv.commands().await;
    assert!(
        cmds.iter()
            .any(|c| c == &vec![json!("set_property"), json!("volume"), json!(37)]),
        "startup pushes Volume=37 into mpv: {cmds:?}"
    );
}

// ---------------------------------------------------------------- keys

#[tokio::test]
#[serial]
async fn minus_and_plus_step_volume_by_one_percent() {
    let mut f = build_remote_app(50).await;
    f.app
        .handle_key(press(KeyCode::Char('-'), KeyModifiers::NONE))
        .await
        .unwrap();
    f.app
        .handle_key(press(KeyCode::Char('+'), KeyModifiers::NONE))
        .await
        .unwrap();
    // Top-row `+` arrives with SHIFT on some terminals; `=` is the
    // unshifted alias.
    f.app
        .handle_key(press(KeyCode::Char('+'), KeyModifiers::SHIFT))
        .await
        .unwrap();
    f.app
        .handle_key(press(KeyCode::Char('='), KeyModifiers::NONE))
        .await
        .unwrap();

    let reqs = f.rec.requests().await;
    let vols: Vec<i32> = reqs
        .iter()
        .filter_map(|r| match r {
            DaemonRequest::SetVolume(v) => Some(*v),
            _ => None,
        })
        .collect();
    assert_eq!(
        vols,
        vec![49, 51, 51, 51],
        "each press is ±1 from the config volume: {reqs:?}"
    );
}

#[tokio::test]
#[serial]
async fn brackets_step_volume_by_five_percent() {
    let mut f = build_remote_app(50).await;
    f.app
        .handle_key(press(KeyCode::Char('['), KeyModifiers::NONE))
        .await
        .unwrap();
    f.app
        .handle_key(press(KeyCode::Char(']'), KeyModifiers::NONE))
        .await
        .unwrap();

    let reqs = f.rec.requests().await;
    let vols: Vec<i32> = reqs
        .iter()
        .filter_map(|r| match r {
            DaemonRequest::SetVolume(v) => Some(*v),
            _ => None,
        })
        .collect();
    assert_eq!(vols, vec![45, 55], "{reqs:?}");
}

#[tokio::test]
#[serial]
async fn volume_keys_stop_at_the_edges() {
    let mut f = build_remote_app(2).await;
    f.app
        .handle_key(press(KeyCode::Char('['), KeyModifiers::NONE))
        .await
        .unwrap();
    let reqs = f.rec.requests().await;
    assert!(
        reqs.iter()
            .any(|r| matches!(r, DaemonRequest::SetVolume(0))),
        "5 % down from 2 % clamps to 0: {reqs:?}"
    );

    let mut g = build_remote_app(99).await;
    g.app
        .handle_key(press(KeyCode::Char(']'), KeyModifiers::NONE))
        .await
        .unwrap();
    let reqs = g.rec.requests().await;
    assert!(
        reqs.iter()
            .any(|r| matches!(r, DaemonRequest::SetVolume(100))),
        "5 % up from 99 % clamps to 100: {reqs:?}"
    );
}

#[tokio::test]
#[serial]
async fn volume_keys_are_typed_into_text_fields_not_intercepted() {
    let mut f = build_remote_app(50).await;
    {
        let mut cs = f.app.client_state.write().await;
        cs.page = ferrosonic::app::state::Page::Server;
        cs.server_state.selected_field = 0;
    }
    f.app
        .handle_key(press(KeyCode::Char('-'), KeyModifiers::NONE))
        .await
        .unwrap();
    let reqs = f.rec.requests().await;
    assert!(
        !reqs
            .iter()
            .any(|r| matches!(r, DaemonRequest::SetVolume(_))),
        "a text field keeps the dash: {reqs:?}"
    );
    let cs = f.app.client_state.read().await;
    assert!(
        cs.server_state.base_url.ends_with('-'),
        "dash typed into the URL field: {:?}",
        cs.server_state.base_url
    );
}

// ---------------------------------------------------------------- event → TUI

#[tokio::test]
#[serial]
async fn volume_changed_event_updates_state_and_arms_the_slider() {
    let f = build_remote_app(50).await;
    let client: Arc<dyn ferrosonic::ipc::client::DaemonClient> = f.rec.clone();
    let cover = common::render::empty_cover_art_state();

    assert!(
        f.app.client_state.read().await.volume_adjusted_at.is_none(),
        "no slider before any change"
    );

    ferrosonic::app::apply_event(
        &f.app.daemon_state,
        &f.app.client_state,
        &client,
        &cover,
        DaemonEvent::VolumeChanged(42),
    )
    .await;

    assert_eq!(f.app.daemon_state.read().await.config.volume, 42);
    let cs = f.app.client_state.read().await;
    assert!(
        cs.volume_adjusted_at.is_some(),
        "a volume change arms the transient slider"
    );
    assert!(
        cs.volume_slider_visible(),
        "slider visible right after a change"
    );
}

#[tokio::test]
#[serial]
async fn slider_hides_two_seconds_after_the_last_change() {
    let f = build_remote_app(50).await;
    let mut cs = f.app.client_state.write().await;
    cs.volume_adjusted_at = Some(Instant::now() - Duration::from_millis(1_900));
    assert!(cs.volume_slider_visible(), "still visible at 1.9 s");
    cs.volume_adjusted_at = Some(Instant::now() - Duration::from_millis(2_100));
    assert!(!cs.volume_slider_visible(), "gone after 2 s");
}

// ---------------------------------------------------------------- rendering

#[test]
fn volume_bar_fills_proportionally_with_fractional_cells() {
    // 10 cells: 72 % = 7.2 cells → 7 full blocks, one 1/8-ish partial, rest empty.
    let bar = volume_bar(72, 10);
    let cells: Vec<char> = bar.chars().collect();
    assert_eq!(cells.len(), 10, "bar is exactly `width` cells: {bar:?}");
    assert!(cells[..7].iter().all(|&c| c == '█'), "{bar:?}");
    assert_eq!(
        cells[7], '▎',
        "0.2 of a cell rounds to two eighths: {bar:?}"
    );
    assert!(cells[8..].iter().all(|&c| c == ' '), "{bar:?}");

    assert_eq!(volume_bar(0, 10), " ".repeat(10));
    assert_eq!(volume_bar(100, 10), "█".repeat(10));
    // 1 % steps stay visible on a narrow bar: 50 → 51 changes the glyph.
    assert_ne!(volume_bar(50, 12), volume_bar(51, 12));
}

async fn seed_playing_song(f: &RemoteApp) {
    let mut ds = f.app.daemon_state.write().await;
    let s = song("s1", "Track");
    ds.now_playing.song = Some(s.clone());
    ds.queue = vec![s];
    ds.queue_position = Some(0);
    ds.now_playing.state = PlaybackState::Playing;
    ds.now_playing.duration = 180.0;
    ds.now_playing.position = 30.0;
}

#[tokio::test]
#[serial]
async fn quality_row_always_shows_the_volume_percent() {
    let f = build_remote_app(72).await;
    seed_playing_song(&f).await;
    let ds = f.app.daemon_state.read().await;
    let mut cs = f.app.client_state.write().await;
    let screen = common::render::render(100, 30, &ds, &mut cs);
    // Right-aligned to 3 digits so 99 → 100 does not shift the centred row.
    assert!(
        screen.contains("♪  72%"),
        "quality row carries the volume:\n{screen}"
    );
    assert!(
        screen.contains("00:30 / 03:00"),
        "progress bar is untouched when not adjusting:\n{screen}"
    );
}

#[tokio::test]
#[serial]
async fn slider_replaces_the_progress_bar_while_adjusting() {
    let f = build_remote_app(72).await;
    seed_playing_song(&f).await;
    let ds = f.app.daemon_state.read().await;
    let mut cs = f.app.client_state.write().await;
    cs.volume_adjusted_at = Some(Instant::now());
    let screen = common::render::render(100, 30, &ds, &mut cs);
    assert!(screen.contains("Vol "), "slider row is labelled:\n{screen}");
    assert!(
        !screen.contains("00:30 / 03:00"),
        "progress bar yields to the slider:\n{screen}"
    );
    assert!(
        screen.contains("█"),
        "slider is drawn with block cells:\n{screen}"
    );
}
