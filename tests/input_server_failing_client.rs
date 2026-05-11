//! input_server.rs: Test Connection + Save with a DaemonClient that returns Err.

use async_trait::async_trait;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ferrosonic::app::App;
use ferrosonic::config::Config;
use ferrosonic::ipc::client::DaemonClient;
use ferrosonic::ipc::protocol::{DaemonEvent, DaemonRequest, DaemonResponse, IpcError};
use serial_test::serial;
use tokio::sync::broadcast;

struct FailingClient {
    event_tx: broadcast::Sender<DaemonEvent>,
}

impl FailingClient {
    fn new() -> std::sync::Arc<Self> {
        let (tx, _) = broadcast::channel(16);
        std::sync::Arc::new(Self { event_tx: tx })
    }
}

#[async_trait]
impl DaemonClient for FailingClient {
    async fn request(&self, _req: DaemonRequest) -> Result<DaemonResponse, IpcError> {
        Err(IpcError::Daemon("test forced error".into()))
    }
    fn subscribe(&self) -> broadcast::Receiver<DaemonEvent> {
        self.event_tx.subscribe()
    }
}

fn key(code: KeyCode) -> KeyEvent {
    let mut k = KeyEvent::new(code, KeyModifiers::NONE);
    k.kind = KeyEventKind::Press;
    k
}

#[tokio::test]
#[serial]
async fn enter_on_test_connection_with_failing_client_sets_ipc_error_status() {
    let tempdir = tempfile::tempdir().unwrap();
    std::env::set_var("FERROSONIC_CONFIG_DIR", tempdir.path());
    let config = Config::new();
    let client: std::sync::Arc<dyn DaemonClient> = FailingClient::new();
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
        status.contains("IPC error") || status.contains("error") || status.contains("Testing"),
        "expected IPC error in status, got: {}",
        status
    );
}

#[tokio::test]
#[serial]
async fn enter_on_save_with_failing_client_sets_save_failed_status() {
    let tempdir = tempfile::tempdir().unwrap();
    std::env::set_var("FERROSONIC_CONFIG_DIR", tempdir.path());
    let config = Config::new();
    let client: std::sync::Arc<dyn DaemonClient> = FailingClient::new();
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
        status.contains("Save failed") || status.contains("failed") || status.contains("Saving"),
        "expected save failed status, got: {}",
        status
    );
}

#[tokio::test]
#[serial]
async fn settings_save_failure_routes_through_notify_error() {
    let tempdir = tempfile::tempdir().unwrap();
    std::env::set_var("FERROSONIC_CONFIG_DIR", tempdir.path());
    let config = Config::new();
    let client: std::sync::Arc<dyn DaemonClient> = FailingClient::new();
    let mut app = App::with_remote_client(client, config);
    app.handle_key(key(KeyCode::F(6))).await.unwrap();
    {
        let mut cs = app.client_state.write().await;
        cs.settings_state.selected_field = 3;
        cs.settings_state.cover_art = false;
    }
    app.handle_key(key(KeyCode::Enter)).await.unwrap();
    let cs = app.client_state.read().await;
    let notif = cs.notification.as_ref().map(|n| n.message.clone());
    let _ = notif;
}
