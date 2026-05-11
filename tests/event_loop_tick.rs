//! Test seams from the App::event_loop refactor.

use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ferrosonic::app::App;
use ferrosonic::config::Config;
use ratatui::backend::TestBackend;
use ratatui::Terminal;
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
    let app = App::new(config);
    AppFixture {
        app,
        _tempdir: tempdir,
    }
}

#[tokio::test]
#[serial]
async fn draw_once_into_test_backend_renders_without_panic() {
    let mut fx = build_app().await;
    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).unwrap();
    fx.app.draw_once(&mut terminal).await.unwrap();
}

#[tokio::test]
#[serial]
async fn should_quit_reflects_client_state_flag() {
    let fx = build_app().await;
    assert!(!fx.app.should_quit().await);
    {
        let mut cs = fx.app.client_state.write().await;
        cs.should_quit = true;
    }
    assert!(fx.app.should_quit().await);
}

#[tokio::test]
#[serial]
async fn q_keypress_triggers_quit_via_handle_event() {
    let mut fx = build_app().await;
    fx.app
        .handle_event(Event::Key(key(KeyCode::Char('q'))))
        .await
        .unwrap();
    assert!(fx.app.should_quit().await);
}

#[tokio::test]
#[serial]
async fn tick_post_expires_old_notifications() {
    let mut fx = build_app().await;
    {
        let mut cs = fx.app.client_state.write().await;
        cs.notify("transient");
        if let Some(ref mut n) = cs.notification {
            n.created_at = std::time::Instant::now() - std::time::Duration::from_secs(5);
        }
    }
    fx.app.tick_post().await;
    let cs = fx.app.client_state.read().await;
    assert!(
        cs.notification.is_none(),
        "tick_post should expire old notifications"
    );
}

#[tokio::test]
#[serial]
async fn fresh_notification_survives_tick_post() {
    let mut fx = build_app().await;
    {
        let mut cs = fx.app.client_state.write().await;
        cs.notify("recent");
    }
    fx.app.tick_post().await;
    let cs = fx.app.client_state.read().await;
    assert!(
        cs.notification.is_some(),
        "fresh notifications must survive a tick"
    );
}

#[tokio::test]
#[serial]
async fn simulated_event_loop_quits_when_quit_event_fires() {
    let mut fx = build_app().await;
    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).unwrap();

    fx.app.draw_once(&mut terminal).await.unwrap();
    assert!(!fx.app.should_quit().await);

    fx.app
        .handle_event(Event::Key(key(KeyCode::Char('q'))))
        .await
        .unwrap();

    fx.app.draw_once(&mut terminal).await.unwrap();
    assert!(
        fx.app.should_quit().await,
        "simulated loop must terminate after q"
    );
}
