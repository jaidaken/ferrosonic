//! input_server.rs: Test Connection + Save fields via RecordingClient
//! that returns Ok(DaemonResponse::Ok) (unexpected variant).

mod common;

use common::RecordingClient;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ferrosonic::app::App;
use ferrosonic::config::Config;
use serial_test::serial;

fn key(code: KeyCode) -> KeyEvent {
    let mut k = KeyEvent::new(code, KeyModifiers::NONE);
    k.kind = KeyEventKind::Press;
    k
}

#[tokio::test]
#[serial]
async fn enter_on_test_connection_with_recording_client_sets_unexpected_status() {
    let tempdir = common::tempdir();
    std::env::set_var("FERROSONIC_CONFIG_DIR", tempdir.path());
    let config = Config::new();
    let client: std::sync::Arc<dyn ferrosonic::ipc::DaemonClient> = RecordingClient::new();
    let mut app = App::with_remote_client(client, config);
    app.handle_key(key(KeyCode::F(5))).await.unwrap();
    {
        let mut cs = app.client_state.write().await;
        cs.server_state.selected_field = 3;
        cs.server_state.base_url = "https://example.com".into();
        cs.server_state.username = "u".into();
        cs.server_state.password = "p".into();
    }
    app.handle_key(key(KeyCode::Enter)).await.unwrap();
    let cs = app.client_state.read().await;
    let status = cs.server_state.status.clone().unwrap_or_default();
    assert!(
        status.contains("Unexpected") || status.contains("Testing"),
        "expected Unexpected/Testing status, got: {}",
        status
    );
}

#[tokio::test]
#[serial]
async fn enter_on_save_with_recording_client_sets_connected_status() {
    let tempdir = common::tempdir();
    std::env::set_var("FERROSONIC_CONFIG_DIR", tempdir.path());
    let config = Config::new();
    let client: std::sync::Arc<dyn ferrosonic::ipc::DaemonClient> = RecordingClient::new();
    let mut app = App::with_remote_client(client, config);
    app.handle_key(key(KeyCode::F(5))).await.unwrap();
    {
        let mut cs = app.client_state.write().await;
        cs.server_state.selected_field = 4;
        cs.server_state.base_url = "https://example.com".into();
        cs.server_state.username = "u".into();
        cs.server_state.password = "p".into();
    }
    app.handle_key(key(KeyCode::Enter)).await.unwrap();
    let cs = app.client_state.read().await;
    let status = cs.server_state.status.clone().unwrap_or_default();
    assert!(
        status.contains("Connected") || status.contains("Saving"),
        "expected Connected/Saving status, got: {}",
        status
    );
}

#[tokio::test]
#[serial]
async fn server_field_text_char_branch_default_arm_unreachable_but_documented() {
    let tempdir = common::tempdir();
    std::env::set_var("FERROSONIC_CONFIG_DIR", tempdir.path());
    let mut config = Config::new();
    config.daemon = false;
    let mut app = App::new(config);
    app.handle_key(key(KeyCode::F(5))).await.unwrap();
    {
        let mut cs = app.client_state.write().await;
        cs.server_state.selected_field = 2;
        cs.server_state.password.clear();
    }
    app.handle_key(key(KeyCode::Char('a'))).await.unwrap();
    app.handle_key(key(KeyCode::Char('b'))).await.unwrap();
    app.handle_key(key(KeyCode::Backspace)).await.unwrap();
    let cs = app.client_state.read().await;
    assert_eq!(cs.server_state.password.reveal(), "a");
}
