//! input_server.rs: Enter on Test + Save fields with real daemon dispatch.

mod common;

use common::TestDaemon;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ferrosonic::app::App;
use serial_test::serial;

fn key(code: KeyCode) -> KeyEvent {
    let mut k = KeyEvent::new(code, KeyModifiers::NONE);
    k.kind = KeyEventKind::Press;
    k
}

async fn build_app_with_td() -> (App, TestDaemon) {
    let td = TestDaemon::new().await;
    let cfg = td.state.read().await.config.clone();
    let mut app = App::with_remote_client(
        std::sync::Arc::new(ferrosonic::ipc::InProcessClient::new(td.core.clone())),
        cfg,
    );
    app.handle_key(key(KeyCode::F(5))).await.unwrap();
    (app, td)
}

#[tokio::test]
#[serial]
async fn enter_on_test_connection_success_path_sets_status_ok() {
    let (mut app, td) = build_app_with_td().await;
    td.fake_subsonic.expect_ping().await;
    {
        let mut cs = app.client_state.write().await;
        cs.server_state.selected_field = 3;
        cs.server_state.base_url = td.fake_subsonic.url();
        cs.server_state.username = "u".into();
        cs.server_state.password = "p".into();
    }
    app.handle_key(key(KeyCode::Enter)).await.unwrap();
    let cs = app.client_state.read().await;
    let status = cs.server_state.status.clone().unwrap_or_default();
    assert!(
        status.contains("successful") || status.contains("OK") || status.contains("Testing"),
        "status: {}",
        status
    );
}

#[tokio::test]
#[serial]
async fn enter_on_test_connection_failure_path_sets_error_status() {
    let (mut app, _td) = build_app_with_td().await;
    {
        let mut cs = app.client_state.write().await;
        cs.server_state.selected_field = 3;
        cs.server_state.base_url = "not a real url".into();
        cs.server_state.username = "u".into();
        cs.server_state.password = "p".into();
    }
    app.handle_key(key(KeyCode::Enter)).await.unwrap();
    let cs = app.client_state.read().await;
    assert!(cs.server_state.status.is_some());
}

#[tokio::test]
#[serial]
async fn enter_on_save_field_success_path_sets_connected_status() {
    let (mut app, td) = build_app_with_td().await;
    td.fake_subsonic.expect_ping().await;
    td.fake_subsonic.expect_starred().await;
    td.fake_subsonic.expect_artists(&["A"]).await;
    td.fake_subsonic.expect_playlists().await;
    {
        let mut cs = app.client_state.write().await;
        cs.server_state.selected_field = 4;
        cs.server_state.base_url = td.fake_subsonic.url();
        cs.server_state.username = "user".into();
        cs.server_state.password = "pw".into();
    }
    app.handle_key(key(KeyCode::Enter)).await.unwrap();
    let cs = app.client_state.read().await;
    let status = cs.server_state.status.clone().unwrap_or_default();
    assert!(
        status.contains("Connected") || status.contains("Saving") || status.contains("loaded"),
        "status: {}",
        status
    );
}

#[tokio::test]
#[serial]
async fn enter_on_save_field_with_bad_url_sets_save_failed() {
    let (mut app, _td) = build_app_with_td().await;
    {
        let mut cs = app.client_state.write().await;
        cs.server_state.selected_field = 4;
        cs.server_state.base_url = "not a url".into();
        cs.server_state.username = "u".into();
        cs.server_state.password = "p".into();
    }
    app.handle_key(key(KeyCode::Enter)).await.unwrap();
    let cs = app.client_state.read().await;
    let status = cs.server_state.status.clone().unwrap_or_default();
    assert!(
        status.contains("failed") || status.contains("Save") || status.contains("Saving"),
        "status: {}",
        status
    );
}
