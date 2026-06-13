//! Quit-confirm modal: `q` opens it only when daemon-backed, and y/n/esc
//! drive the prompt state machine (src/app/input.rs handle_key).

use std::time::Duration;

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

async fn build_app() -> App {
    let mut config = Config::new();
    config.daemon = false;
    let tempdir = tempfile::tempdir().unwrap();
    std::env::set_var("FERROSONIC_CONFIG_DIR", tempdir.path());
    std::mem::forget(tempdir);
    App::new(config)
}

#[tokio::test]
#[serial]
async fn q_when_daemon_backed_opens_prompt_without_quitting() {
    let mut app = build_app().await;
    app.client_state.write().await.daemon_backed = true;

    app.handle_key(key(KeyCode::Char('q'))).await.unwrap();

    let cs = app.client_state.read().await;
    assert!(cs.quit_prompt, "daemon-backed q must raise the confirm prompt");
    assert!(
        !cs.should_quit,
        "daemon-backed q must not quit before the user confirms"
    );
}

#[tokio::test]
#[serial]
async fn q_without_daemon_quits_immediately_without_prompt() {
    let mut app = build_app().await;
    assert!(
        !app.client_state.read().await.daemon_backed,
        "in-process build is not daemon-backed"
    );

    app.handle_key(key(KeyCode::Char('q'))).await.unwrap();

    let cs = app.client_state.read().await;
    assert!(cs.should_quit, "standalone q must quit at once");
    assert!(!cs.quit_prompt, "standalone q must not raise a prompt");
}

#[tokio::test]
#[serial]
async fn n_at_prompt_quits_and_clears_prompt() {
    let mut app = build_app().await;
    {
        let mut cs = app.client_state.write().await;
        cs.daemon_backed = true;
        cs.quit_prompt = true;
    }

    app.handle_key(key(KeyCode::Char('n'))).await.unwrap();

    let cs = app.client_state.read().await;
    assert!(cs.should_quit, "n quits the TUI");
    assert!(!cs.quit_prompt, "n dismisses the prompt");
}

#[tokio::test]
#[serial]
async fn esc_at_prompt_cancels_without_quitting() {
    let mut app = build_app().await;
    {
        let mut cs = app.client_state.write().await;
        cs.daemon_backed = true;
        cs.quit_prompt = true;
    }

    app.handle_key(key(KeyCode::Esc)).await.unwrap();

    let cs = app.client_state.read().await;
    assert!(!cs.quit_prompt, "esc dismisses the prompt");
    assert!(!cs.should_quit, "esc keeps the TUI running");
}

#[tokio::test]
#[serial]
async fn unrelated_key_at_prompt_is_swallowed() {
    let mut app = build_app().await;
    {
        let mut cs = app.client_state.write().await;
        cs.daemon_backed = true;
        cs.quit_prompt = true;
    }

    app.handle_key(key(KeyCode::F(6))).await.unwrap();

    let cs = app.client_state.read().await;
    assert_eq!(
        cs.page,
        Page::Library,
        "F6 must not switch pages while the prompt is up"
    );
    assert!(cs.quit_prompt, "an unrelated key leaves the prompt open");
    assert!(!cs.should_quit, "an unrelated key does not quit");
}

#[tokio::test]
#[serial]
async fn y_at_prompt_quits_and_clears_prompt() {
    let mut app = build_app().await;
    {
        let mut cs = app.client_state.write().await;
        cs.daemon_backed = true;
        cs.quit_prompt = true;
    }

    tokio::time::timeout(Duration::from_secs(10), app.handle_key(key(KeyCode::Char('y'))))
        .await
        .expect("y handler must not hang on the shutdown request")
        .unwrap();

    let cs = app.client_state.read().await;
    assert!(cs.should_quit, "y quits the TUI");
    assert!(!cs.quit_prompt, "y dismisses the prompt");
}
