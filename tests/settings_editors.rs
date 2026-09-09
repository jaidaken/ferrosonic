//! In-app playback-filter and global-keybinding editor regressions.

mod common;

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ferrosonic::app::App;
use ferrosonic::config::keybind::{GlobalAction, KeyChord};
use ferrosonic::config::Config;
use ferrosonic::ipc::client::DaemonClient;
use ferrosonic::ipc::protocol::{DaemonEvent, DaemonRequest, DaemonResponse, IpcError};
use serial_test::serial;
use std::sync::Arc;
use tokio::sync::broadcast;

struct FailingClient {
    events: broadcast::Sender<DaemonEvent>,
}

impl FailingClient {
    fn new() -> Arc<Self> {
        let (events, _) = broadcast::channel(4);
        Arc::new(Self { events })
    }
}

#[async_trait::async_trait]
impl DaemonClient for FailingClient {
    async fn request(&self, _request: DaemonRequest) -> Result<DaemonResponse, IpcError> {
        Err(IpcError::Daemon("write failed".into()))
    }

    fn subscribe(&self) -> broadcast::Receiver<DaemonEvent> {
        self.events.subscribe()
    }
}

fn key(code: KeyCode) -> KeyEvent {
    modified_key(code, KeyModifiers::NONE)
}

fn modified_key(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
    let mut key = KeyEvent::new(code, modifiers);
    key.kind = KeyEventKind::Press;
    key
}

async fn app_on_settings() -> (App, tempfile::TempDir) {
    let tempdir = common::tempdir();
    std::env::set_var("FERROSONIC_CONFIG_DIR", tempdir.path());
    let mut config = Config::new();
    config.daemon = false;
    let mut app = App::new(config);
    app.handle_key(key(KeyCode::F(6))).await.unwrap();
    (app, tempdir)
}

#[tokio::test]
#[serial]
async fn genre_editor_trims_deduplicates_and_persists_on_save() {
    let (mut app, _tempdir) = app_on_settings().await;
    app.client_state.write().await.settings_state.selected_field = 18;
    app.handle_key(key(KeyCode::Enter)).await.unwrap();
    app.handle_key(key(KeyCode::Char('a'))).await.unwrap();
    for character in "  Podcast  ".chars() {
        app.handle_key(key(KeyCode::Char(character))).await.unwrap();
    }
    app.handle_key(key(KeyCode::Enter)).await.unwrap();

    // A differently-cased duplicate remains in add mode and does not alter the draft.
    app.handle_key(key(KeyCode::Char('a'))).await.unwrap();
    for character in "podcast".chars() {
        app.handle_key(key(KeyCode::Char(character))).await.unwrap();
    }
    app.handle_key(key(KeyCode::Enter)).await.unwrap();
    {
        let cs = app.client_state.read().await;
        let editor = cs.settings_state.filter_editor.as_ref().unwrap();
        assert_eq!(editor.entries, vec!["Podcast"]);
        assert!(editor.adding);
    }
    app.handle_key(key(KeyCode::Esc)).await.unwrap();
    app.handle_key(modified_key(KeyCode::Char('s'), KeyModifiers::CONTROL))
        .await
        .unwrap();

    let cs = app.client_state.read().await;
    assert!(cs.settings_state.filter_editor.is_none());
    assert_eq!(
        cs.settings_state.playback_filters.excluded_genres,
        vec!["Podcast"]
    );
    drop(cs);
    let persisted = Config::load_default().unwrap();
    assert_eq!(persisted.playback_filters.excluded_genres, vec!["Podcast"]);
}

#[tokio::test]
#[serial]
async fn escape_cancels_filter_edits_without_mutating_active_filters() {
    let (mut app, _tempdir) = app_on_settings().await;
    {
        let mut cs = app.client_state.write().await;
        cs.settings_state.selected_field = 19;
        cs.settings_state.playback_filters.excluded_artists = vec!["Existing".into()];
    }
    app.handle_key(key(KeyCode::Enter)).await.unwrap();
    app.handle_key(key(KeyCode::Char('d'))).await.unwrap();
    app.handle_key(key(KeyCode::Esc)).await.unwrap();

    let cs = app.client_state.read().await;
    assert!(cs.settings_state.filter_editor.is_none());
    assert_eq!(
        cs.settings_state.playback_filters.excluded_artists,
        vec!["Existing"]
    );
}

#[tokio::test]
#[serial]
async fn keybinding_editor_persists_and_applies_new_quit_key_immediately() {
    let (mut app, _tempdir) = app_on_settings().await;
    app.client_state.write().await.settings_state.selected_field = 20;
    app.handle_key(key(KeyCode::Enter)).await.unwrap();
    app.handle_key(key(KeyCode::Enter)).await.unwrap();
    app.handle_key(key(KeyCode::Char('z'))).await.unwrap();
    app.handle_key(modified_key(KeyCode::Char('s'), KeyModifiers::CONTROL))
        .await
        .unwrap();

    let persisted = Config::load_default().unwrap();
    assert_eq!(
        persisted.keybindings.get(&GlobalAction::Quit),
        Some(&"z".parse::<KeyChord>().unwrap())
    );
    app.handle_key(key(KeyCode::Char('z'))).await.unwrap();
    assert!(app.client_state.read().await.should_quit);
}

#[tokio::test]
#[serial]
async fn keybinding_editor_rejects_a_collision_and_keeps_capturing() {
    let (mut app, _tempdir) = app_on_settings().await;
    app.client_state.write().await.settings_state.selected_field = 20;
    app.handle_key(key(KeyCode::Enter)).await.unwrap();
    app.handle_key(key(KeyCode::Enter)).await.unwrap();
    app.handle_key(key(KeyCode::Char('l'))).await.unwrap();

    let cs = app.client_state.read().await;
    let editor = cs.settings_state.keybinding_editor.as_ref().unwrap();
    assert!(editor.capturing);
    assert!(!editor.bindings.contains_key(&GlobalAction::Quit));
    assert!(cs.notification.as_ref().is_some_and(|n| n.is_error));
}

#[tokio::test]
async fn failed_keybinding_save_keeps_the_active_map_and_staged_editor() {
    let mut app = App::with_remote_client(FailingClient::new(), Config::new());
    {
        let mut cs = app.client_state.write().await;
        cs.page = ferrosonic::app::state::Page::Settings;
        cs.settings_state.selected_field = 20;
    }
    app.handle_key(key(KeyCode::Enter)).await.unwrap();
    app.handle_key(key(KeyCode::Enter)).await.unwrap();
    app.handle_key(key(KeyCode::Char('z'))).await.unwrap();
    app.handle_key(modified_key(KeyCode::Char('s'), KeyModifiers::CONTROL))
        .await
        .unwrap();
    {
        let cs = app.client_state.read().await;
        assert!(cs.settings_state.keybindings.is_empty());
        assert!(cs.settings_state.keybinding_editor.is_some());
        assert!(cs.notification.as_ref().is_some_and(|n| n.is_error));
    }
    app.handle_key(key(KeyCode::Esc)).await.unwrap();
    app.handle_key(key(KeyCode::Char('z'))).await.unwrap();
    assert!(!app.client_state.read().await.should_quit);
}

#[tokio::test]
#[serial]
async fn capture_accepts_ctrl_s_before_ctrl_s_saves_the_editor() {
    let (mut app, _tempdir) = app_on_settings().await;
    app.client_state.write().await.settings_state.selected_field = 20;
    app.handle_key(key(KeyCode::Enter)).await.unwrap();
    app.handle_key(key(KeyCode::Enter)).await.unwrap();
    let ctrl_s = modified_key(KeyCode::Char('s'), KeyModifiers::CONTROL);
    app.handle_key(ctrl_s).await.unwrap();
    assert!(
        !app.client_state
            .read()
            .await
            .settings_state
            .keybinding_editor
            .as_ref()
            .unwrap()
            .capturing
    );
    app.handle_key(ctrl_s).await.unwrap();
    assert!(app
        .client_state
        .read()
        .await
        .settings_state
        .keybinding_editor
        .is_none());
}
