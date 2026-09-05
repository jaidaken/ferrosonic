//! Configurable global keybindings (Phase 4 / #12b): a `[Keybindings]`
//! override in `Config` actually changes which key fires which global
//! action, the defaults are unaffected when no override is present, and
//! the whole thing round-trips through TOML.

mod common;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ferrosonic::app::App;
use ferrosonic::config::keybind::{GlobalAction, KeyChord};
use ferrosonic::config::Config;
use ferrosonic::ipc::client::DaemonClient;
use ferrosonic::ipc::protocol::{DaemonEvent, DaemonRequest, DaemonResponse, IpcError};
use serial_test::serial;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::broadcast;

struct RecordingClient {
    event_tx: broadcast::Sender<DaemonEvent>,
    requests: Mutex<Vec<DaemonRequest>>,
}

impl RecordingClient {
    fn new() -> Arc<Self> {
        let (tx, _) = broadcast::channel(16);
        Arc::new(Self {
            event_tx: tx,
            requests: Mutex::new(Vec::new()),
        })
    }
    fn sent(&self) -> Vec<DaemonRequest> {
        self.requests.lock().unwrap().clone()
    }
}

#[async_trait::async_trait]
impl DaemonClient for RecordingClient {
    async fn request(&self, req: DaemonRequest) -> Result<DaemonResponse, IpcError> {
        self.requests.lock().unwrap().push(req);
        Ok(DaemonResponse::Ok)
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

async fn press(app: &mut App, k: KeyEvent) {
    app.handle_key(k).await.unwrap();
}

#[tokio::test]
#[serial]
async fn a_custom_override_moves_next_track_to_its_new_key() {
    let mut config = Config::new();
    config
        .keybindings
        .insert(GlobalAction::NextTrack, "j".parse::<KeyChord>().unwrap());

    let client = RecordingClient::new();
    let mut app = App::with_remote_client(client.clone(), config);

    press(&mut app, key(KeyCode::Char('j'))).await;
    assert!(
        client
            .sent()
            .iter()
            .any(|r| matches!(r, DaemonRequest::Next)),
        "the remapped key 'j' must now fire NextTrack"
    );
}

#[tokio::test]
#[serial]
async fn remapping_next_track_away_frees_its_old_default_key() {
    let mut config = Config::new();
    config
        .keybindings
        .insert(GlobalAction::NextTrack, "j".parse::<KeyChord>().unwrap());

    let client = RecordingClient::new();
    let mut app = App::with_remote_client(client.clone(), config);

    press(&mut app, key(KeyCode::Char('l'))).await;
    assert!(
        !client
            .sent()
            .iter()
            .any(|r| matches!(r, DaemonRequest::Next)),
        "'l' must no longer fire NextTrack once the binding moved to 'j'"
    );
}

#[tokio::test]
#[serial]
async fn remapping_a_page_switch_still_reverts_unsaved_edits() {
    // GoToLibrary's cleanup logic (input.rs's is_page_switch branch) must
    // follow the resolved action, not the literal F1 key, or a remapped
    // page-switch key would silently skip the "revert unsaved edits" step.
    let mut config = Config::new();
    config
        .keybindings
        .insert(GlobalAction::GoToLibrary, "z".parse::<KeyChord>().unwrap());

    let client = RecordingClient::new();
    let mut app = App::with_remote_client(client.clone(), config);
    {
        let mut cs = app.client_state.write().await;
        cs.page = ferrosonic::app::state::Page::Playlists;
        cs.playlists.renaming = true;
        cs.playlists.rename_buf = "x".into();
    }

    press(&mut app, key(KeyCode::Char('z'))).await;

    let cs = app.client_state.read().await;
    assert_eq!(cs.page, ferrosonic::app::state::Page::Library);
    assert!(
        !cs.playlists.renaming,
        "the remapped page-switch key must still cancel the playlist rename overlay"
    );
}

#[tokio::test]
#[serial]
async fn p_still_pauses_even_when_toggle_pause_is_remapped_away_from_space() {
    // 'p' is a fixed secondary alias for TogglePause, kept outside the
    // configurable keymap so it survives remapping the primary (Space) slot.
    let mut config = Config::new();
    config
        .keybindings
        .insert(GlobalAction::TogglePause, "x".parse::<KeyChord>().unwrap());

    let client = RecordingClient::new();
    let mut app = App::with_remote_client(client.clone(), config);

    press(&mut app, key(KeyCode::Char('p'))).await;
    assert!(
        client
            .sent()
            .iter()
            .any(|r| matches!(r, DaemonRequest::TogglePause)),
        "'p' must still pause regardless of where TogglePause's configurable slot points"
    );
}

#[tokio::test]
#[serial]
async fn empty_keybindings_config_produces_the_documented_defaults() {
    // Regression safety net: an absent/empty [Keybindings] table must
    // dispatch identically to the hardcoded pre-Phase-4 bindings.
    let client = RecordingClient::new();
    let mut app = App::with_remote_client(client.clone(), Config::new());

    press(&mut app, key(KeyCode::Char('l'))).await;
    assert!(client
        .sent()
        .iter()
        .any(|r| matches!(r, DaemonRequest::Next)));

    press(&mut app, key(KeyCode::Char('h'))).await;
    assert!(client
        .sent()
        .iter()
        .any(|r| matches!(r, DaemonRequest::Previous)));

    press(&mut app, key(KeyCode::F(2))).await;
    assert_eq!(
        app.client_state.read().await.page,
        ferrosonic::app::state::Page::Queue
    );
}

#[test]
fn keybindings_override_round_trips_through_toml() {
    let mut config = Config::new();
    config
        .keybindings
        .insert(GlobalAction::NextTrack, "j".parse::<KeyChord>().unwrap());
    config
        .keybindings
        .insert(GlobalAction::Refresh, "Ctrl+e".parse::<KeyChord>().unwrap());

    let dir = common::tempdir();
    let path = dir.path().join("c.toml");
    config.save_to_file(&path).unwrap();

    let loaded = Config::load_from_file(&path).unwrap();
    assert_eq!(
        loaded.keybindings.get(&GlobalAction::NextTrack),
        Some(&"j".parse::<KeyChord>().unwrap())
    );
    assert_eq!(
        loaded.keybindings.get(&GlobalAction::Refresh),
        Some(&"Ctrl+e".parse::<KeyChord>().unwrap())
    );
    assert_eq!(
        loaded.keybindings.len(),
        2,
        "only the two overridden actions should round-trip, not the full default set"
    );
}

#[test]
fn empty_keybindings_are_omitted_from_the_saved_toml() {
    let config = Config::new();
    let dir = common::tempdir();
    let path = dir.path().join("c.toml");
    config.save_to_file(&path).unwrap();

    let contents = std::fs::read_to_string(&path).unwrap();
    assert!(
        !contents.contains("Keybindings"),
        "an empty override map must not write an empty [Keybindings] table"
    );
}

#[test]
fn unknown_keybindings_value_is_rejected_not_silently_dropped() {
    let dir = common::tempdir();
    let path = dir.path().join("c.toml");
    std::fs::write(&path, "[Keybindings]\nNextTrack = \"NotAKey!!\"\n").unwrap();

    assert!(
        Config::load_from_file(&path).is_err(),
        "a malformed key-chord string must fail to load, not silently become a default"
    );
}

fn map_with(action: GlobalAction, chord: &str) -> HashMap<GlobalAction, KeyChord> {
    let mut m = HashMap::new();
    m.insert(action, chord.parse().unwrap());
    m
}

#[test]
fn resolve_end_to_end_from_a_config_style_override_map() {
    use ferrosonic::config::keybind::resolve;
    let overrides = map_with(GlobalAction::ShuffleLibrary, "s");
    let (resolved, _warnings) = resolve(&overrides);
    assert_eq!(
        resolved.get(&"s".parse::<KeyChord>().unwrap()),
        Some(&GlobalAction::ShuffleLibrary)
    );
}

#[test]
fn equivalent_shift_spellings_and_reserved_conflicts() {
    use ferrosonic::config::keybind::{resolve, GlobalAction, KeyChord};
    use std::collections::HashMap;
    assert_eq!("Shift+t".parse::<KeyChord>().unwrap(), "T".parse().unwrap());
    for key in ["p", "1", "2", "3", "4", "5"] {
        let chord = key.parse::<KeyChord>().unwrap();
        let (resolved, warnings) = resolve(&HashMap::from([(GlobalAction::Quit, chord)]));
        assert!(!resolved.contains_key(&chord));
        assert!(warnings
            .iter()
            .any(|w| w.contains("reserved") && w.contains("Quit")));
    }
}
