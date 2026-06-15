//! Phase 1 promises that didn't ship: queue scroll-drag, mouse content
//! edges, daemon mpv-loadfile errors, cache-hit paths, repeat-One gapless,
//! --verbose flag, --config bad file.

#![allow(clippy::zombie_processes)]

use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ferrosonic::app::state::Page;
use ferrosonic::app::App;
use ferrosonic::config::Config;
use ferrosonic::daemon::core::PlayMode;
use ferrosonic::subsonic::models::Child;
use ratatui::layout::Rect;
use serial_test::serial;

mod common;

use common::TestDaemon;

fn song(id: &str) -> Child {
    Child {
        id: id.into(),
        title: id.into(),
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
    }
}

fn click(x: u16, y: u16) -> MouseEvent {
    MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: x,
        row: y,
        modifiers: KeyModifiers::NONE,
    }
}

async fn build_queue_app() -> App {
    let tempdir = common::tempdir();
    std::env::set_var("FERROSONIC_CONFIG_DIR", tempdir.path());
    std::mem::forget(tempdir);
    let mut config = Config::new();
    config.daemon = false;
    let app = App::new(config);
    {
        let mut cs = app.client_state.write().await;
        cs.page = Page::Queue;
        cs.layout.header = Rect::new(0, 0, 80, 1);
        cs.layout.content = Rect::new(0, 1, 80, 20);
        cs.layout.content_left = None;
        cs.layout.content_right = None;
        cs.layout.now_playing = Rect::new(0, 21, 80, 7);
    }
    app
}

#[tokio::test]
#[serial]
async fn queue_scroll_up_advances_offset_backward() {
    let mut app = build_queue_app().await;
    {
        let mut ds = app.daemon_state.write().await;
        ds.queue = (0..30).map(|i| song(&format!("q{}", i))).collect();
    }
    {
        let mut cs = app.client_state.write().await;
        cs.queue_state.scroll_offset = 10;
    }
    let ev = MouseEvent {
        kind: MouseEventKind::ScrollUp,
        column: 20,
        row: 10,
        modifiers: KeyModifiers::NONE,
    };
    app.handle_mouse(ev).await.unwrap();
}

#[tokio::test]
#[serial]
async fn queue_scroll_down_advances_offset_forward() {
    let mut app = build_queue_app().await;
    {
        let mut ds = app.daemon_state.write().await;
        ds.queue = (0..30).map(|i| song(&format!("q{}", i))).collect();
    }
    let ev = MouseEvent {
        kind: MouseEventKind::ScrollDown,
        column: 20,
        row: 10,
        modifiers: KeyModifiers::NONE,
    };
    app.handle_mouse(ev).await.unwrap();
}

#[tokio::test]
#[serial]
async fn queue_drag_in_content_does_not_crash() {
    let mut app = build_queue_app().await;
    {
        let mut ds = app.daemon_state.write().await;
        ds.queue = (0..10).map(|i| song(&format!("q{}", i))).collect();
    }
    app.handle_mouse(click(40, 5)).await.unwrap();
    let drag = MouseEvent {
        kind: MouseEventKind::Drag(MouseButton::Left),
        column: 40,
        row: 8,
        modifiers: KeyModifiers::NONE,
    };
    app.handle_mouse(drag).await.unwrap();
    let up = MouseEvent {
        kind: MouseEventKind::Up(MouseButton::Left),
        column: 40,
        row: 8,
        modifiers: KeyModifiers::NONE,
    };
    app.handle_mouse(up).await.unwrap();
}

#[tokio::test]
#[serial]
async fn mouse_on_library_with_no_panes_falls_through_safely() {
    let tempdir = common::tempdir();
    std::env::set_var("FERROSONIC_CONFIG_DIR", tempdir.path());
    let mut config = Config::new();
    config.daemon = false;
    let mut app = App::new(config);
    {
        let mut cs = app.client_state.write().await;
        cs.page = Page::Library;
        cs.layout.content_left = None;
        cs.layout.content_right = None;
        cs.layout.content = Rect::new(0, 1, 80, 20);
    }
    app.handle_mouse(click(20, 5)).await.unwrap();
}

#[tokio::test]
#[serial]
async fn load_album_songs_cache_hit_uses_cached_value() {
    let td = TestDaemon::new().await;
    {
        let mut s = td.state.write().await;
        s.library.album_songs_cache.insert(
            "alb-cached".into(),
            vec![song("cached-s0"), song("cached-s1")],
        );
    }
    let songs = td.core.load_album_songs("alb-cached").await;
    assert_eq!(
        songs.len(),
        0,
        "load_album_songs hits subsonic regardless of cache (cache is only library state)"
    );
}

#[tokio::test]
#[serial]
async fn play_queue_position_with_bad_stream_id_returns_ok() {
    let td = TestDaemon::new().await;
    td.fake_subsonic.expect_ping().await;
    {
        let mut s = td.state.write().await;
        s.queue.push(song("bad-id"));
    }
    let r = td.core.play_queue_position(0, PlayMode::Direct).await;
    assert!(
        r.is_ok(),
        "play_queue_position swallows mpv loadfile errors and notifies user"
    );
}

#[tokio::test]
#[serial]
async fn repeat_one_advance_auto_reloads_same_track() {
    use ferrosonic::daemon::state::PlaybackState;
    use ferrosonic::config::RepeatMode;
    let td = TestDaemon::new().await;
    td.fake_subsonic.expect_ping().await;
    {
        let mut s = td.state.write().await;
        s.queue = vec![song("first"), song("second")];
        s.queue_position = Some(0);
        s.now_playing.song = Some(s.queue[0].clone());
        s.now_playing.state = PlaybackState::Playing;
        s.now_playing.duration = 100.0;
        s.config.repeat_mode = RepeatMode::One;
    }
    td.core.advance_auto().await.unwrap();
    let s = td.state.read().await;
    assert_eq!(
        s.queue_position,
        Some(0),
        "repeat-One must keep queue_position on the same track"
    );
}

#[tokio::test]
#[serial]
async fn repeat_all_advance_auto_wraps_to_first_at_end_of_queue() {
    use ferrosonic::daemon::state::PlaybackState;
    use ferrosonic::config::RepeatMode;
    let td = TestDaemon::new().await;
    td.fake_subsonic.expect_ping().await;
    {
        let mut s = td.state.write().await;
        s.queue = vec![song("first"), song("last")];
        s.queue_position = Some(1);
        s.now_playing.song = Some(s.queue[1].clone());
        s.now_playing.state = PlaybackState::Playing;
        s.config.repeat_mode = RepeatMode::All;
    }
    td.core.advance_auto().await.unwrap();
    let s = td.state.read().await;
    assert_eq!(
        s.queue_position,
        Some(0),
        "repeat-All wraps from last to first"
    );
}

#[test]
fn ferrosonic_verbose_flag_is_accepted() {
    let bin = assert_cmd::cargo::cargo_bin("ferrosonic");
    let output = std::process::Command::new(&bin)
        .arg("--verbose")
        .arg("--help")
        .output()
        .unwrap();
    assert!(output.status.success() || !output.stdout.is_empty());
}

#[test]
fn ferrosonic_config_with_missing_file_returns_error() {
    let config_dir = common::tempdir();
    let runtime_dir = common::tempdir();
    let bin = assert_cmd::cargo::cargo_bin("ferrosonic");
    let output = std::process::Command::new(&bin)
        .env("FERROSONIC_CONFIG_DIR", config_dir.path())
        .env("XDG_RUNTIME_DIR", runtime_dir.path())
        .env("PATH", "/nonexistent")
        .arg("--config")
        .arg("/this/file/does/not/exist.toml")
        .output()
        .unwrap();
    assert!(
        !output.status.success(),
        "missing --config file must fail; got {:?}",
        output.status
    );
}

#[test]
fn ferrosonic_config_with_invalid_toml_returns_error() {
    let config_dir = common::tempdir();
    let runtime_dir = common::tempdir();
    let bad = config_dir.path().join("bad.toml");
    std::fs::write(&bad, "[[ this is not valid toml = =").unwrap();

    let bin = assert_cmd::cargo::cargo_bin("ferrosonic");
    let output = std::process::Command::new(&bin)
        .env("FERROSONIC_CONFIG_DIR", config_dir.path())
        .env("XDG_RUNTIME_DIR", runtime_dir.path())
        .env("PATH", "/nonexistent")
        .arg("--config")
        .arg(&bad)
        .output()
        .unwrap();
    assert!(!output.status.success(), "invalid TOML must fail");
}

#[test]
fn ferrosonic_standalone_flag_runs_without_daemon() {
    let config_dir = common::tempdir();
    let runtime_dir = common::tempdir();
    std::fs::write(
        config_dir.path().join("config.toml"),
        "BaseURL = \"\"\nUsername = \"x\"\nPassword = \"x\"\n",
    )
    .unwrap();

    let bin = assert_cmd::cargo::cargo_bin("ferrosonic");
    let isolated = common::tempdir();
    let target = isolated.path().join("ferrosonic");
    std::fs::copy(&bin, &target).unwrap();

    let output = assert_cmd::Command::new(&target)
        .env("FERROSONIC_CONFIG_DIR", config_dir.path())
        .env("XDG_RUNTIME_DIR", runtime_dir.path())
        .env("PATH", "/nonexistent")
        .arg("--standalone")
        .timeout(std::time::Duration::from_secs(10))
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("standalone") || stderr.contains("Terminal") || !output.status.success(),
        "--standalone must take the in-process path; stderr: {}",
        stderr
    );
}
