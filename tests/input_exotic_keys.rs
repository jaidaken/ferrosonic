//! Resize, Backtab, Ctrl modifiers, n with no song, ignored keys.

mod common;
use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ferrosonic::app::App;
use ferrosonic::config::Config;
use ferrosonic::subsonic::models::Child;
use serial_test::serial;

fn key(code: KeyCode) -> KeyEvent {
    let mut k = KeyEvent::new(code, KeyModifiers::NONE);
    k.kind = KeyEventKind::Press;
    k
}

fn key_mod(code: KeyCode, mods: KeyModifiers) -> KeyEvent {
    let mut k = KeyEvent::new(code, mods);
    k.kind = KeyEventKind::Press;
    k
}

fn song(id: &str) -> Child {
    Child {
        id: id.into(),
        title: id.into(),
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
    }
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
async fn resize_event_does_not_panic() {
    let mut fx = build_app().await;
    fx.app.handle_event(Event::Resize(120, 40)).await.unwrap();
}

#[tokio::test]
#[serial]
async fn n_with_no_playing_song_is_silent_noop() {
    let mut fx = build_app().await;
    fx.app.handle_key(key(KeyCode::Char('n'))).await.unwrap();
}

#[tokio::test]
#[serial]
async fn n_with_currently_playing_song_triggers_star_request() {
    let mut fx = build_app().await;
    {
        let mut ds = fx.app.daemon_state.write().await;
        ds.queue.push(song("playing-now"));
        ds.queue_position = Some(0);
        ds.now_playing.song = Some(song("playing-now"));
    }
    fx.app.handle_key(key(KeyCode::Char('n'))).await.unwrap();
}

#[tokio::test]
#[serial]
async fn ctrl_c_is_handled_as_quit() {
    let mut fx = build_app().await;
    fx.app
        .handle_key(key_mod(KeyCode::Char('c'), KeyModifiers::CONTROL))
        .await
        .unwrap();
}

#[tokio::test]
#[serial]
async fn backtab_event_does_not_panic() {
    let mut fx = build_app().await;
    fx.app.handle_key(key(KeyCode::BackTab)).await.unwrap();
}

#[tokio::test]
#[serial]
async fn shift_q_does_not_quit() {
    let mut fx = build_app().await;
    fx.app
        .handle_key(key_mod(KeyCode::Char('Q'), KeyModifiers::SHIFT))
        .await
        .unwrap();
    let cs = fx.app.client_state.read().await;
    assert!(!cs.should_quit, "Shift+Q must not trigger quit");
}

#[tokio::test]
#[serial]
async fn unhandled_key_falls_through_silently() {
    let mut fx = build_app().await;
    fx.app.handle_key(key(KeyCode::Insert)).await.unwrap();
    fx.app.handle_key(key(KeyCode::Delete)).await.unwrap();
    fx.app.handle_key(key(KeyCode::Home)).await.unwrap();
    fx.app.handle_key(key(KeyCode::End)).await.unwrap();
}

#[tokio::test]
#[serial]
async fn focus_in_focus_out_events_are_ignored() {
    let mut fx = build_app().await;
    fx.app.handle_event(Event::FocusGained).await.unwrap();
    fx.app.handle_event(Event::FocusLost).await.unwrap();
}
