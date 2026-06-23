//! Global key dispatch in input.rs handle_key: every top-level binding sends the
//! request it advertises, and the F-key / text-field routing guards hold. Uses a
//! RecordingClient so each binding's outbound DaemonRequest is observable.

mod common;
use async_trait::async_trait;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ferrosonic::app::App;
use ferrosonic::config::Config;
use ferrosonic::ipc::client::DaemonClient;
use ferrosonic::ipc::protocol::{DaemonEvent, DaemonRequest, DaemonResponse, IpcError};
use ferrosonic::subsonic::models::Child;
use serial_test::serial;
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

#[async_trait]
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

fn key_mod(code: KeyCode, mods: KeyModifiers) -> KeyEvent {
    let mut k = KeyEvent::new(code, mods);
    k.kind = KeyEventKind::Press;
    k
}

fn song(id: &str) -> Child {
    let mut c = Child::default();
    c.id = id.into();
    c.title = id.into();
    c
}

fn app_with(client: Arc<RecordingClient>) -> App {
    App::with_remote_client(client, Config::new())
}

async fn press(app: &mut App, k: KeyEvent) {
    app.handle_key(k).await.unwrap();
}

#[tokio::test]
#[serial]
async fn p_and_space_send_toggle_pause() {
    for code in [KeyCode::Char('p'), KeyCode::Char(' ')] {
        let client = RecordingClient::new();
        let mut app = app_with(client.clone());
        press(&mut app, key(code)).await;
        assert!(
            client
                .sent()
                .iter()
                .any(|r| matches!(r, DaemonRequest::TogglePause)),
            "{code:?} must send TogglePause"
        );
    }
}

#[tokio::test]
#[serial]
async fn l_sends_next() {
    let client = RecordingClient::new();
    let mut app = app_with(client.clone());
    press(&mut app, key(KeyCode::Char('l'))).await;
    assert!(client
        .sent()
        .iter()
        .any(|r| matches!(r, DaemonRequest::Next)));
}

#[tokio::test]
#[serial]
async fn h_sends_previous() {
    let client = RecordingClient::new();
    let mut app = app_with(client.clone());
    press(&mut app, key(KeyCode::Char('h'))).await;
    assert!(client
        .sent()
        .iter()
        .any(|r| matches!(r, DaemonRequest::Previous)));
}

#[tokio::test]
#[serial]
async fn n_stars_the_now_playing_song() {
    let client = RecordingClient::new();
    let mut app = app_with(client.clone());
    app.daemon_state.write().await.now_playing.song = Some(song("np-1"));
    press(&mut app, key(KeyCode::Char('n'))).await;
    assert!(
        client
            .sent()
            .iter()
            .any(|r| matches!(r, DaemonRequest::ToggleStarSong(id) if id == "np-1")),
        "n must star the now-playing song"
    );
}

#[tokio::test]
#[serial]
async fn capital_t_shuffles_the_library() {
    let client = RecordingClient::new();
    let mut app = app_with(client.clone());
    press(&mut app, key(KeyCode::Char('T'))).await;
    assert!(client
        .sent()
        .iter()
        .any(|r| matches!(r, DaemonRequest::ShuffleLibrary)));
}

#[tokio::test]
#[serial]
async fn r_cycles_repeat_and_sends_set_repeat_mode() {
    let client = RecordingClient::new();
    let mut app = app_with(client.clone());
    let before = app.client_state.read().await.settings_state.repeat_mode;
    press(&mut app, key(KeyCode::Char('r'))).await;
    assert_ne!(
        app.client_state.read().await.settings_state.repeat_mode,
        before,
        "r must cycle the repeat mode"
    );
    assert!(client
        .sent()
        .iter()
        .any(|r| matches!(r, DaemonRequest::SetRepeatMode(_))));
}

#[tokio::test]
#[serial]
async fn ctrl_r_refreshes_and_does_not_cycle_repeat() {
    let client = RecordingClient::new();
    let mut app = app_with(client.clone());
    let before = app.client_state.read().await.settings_state.repeat_mode;
    press(&mut app, key_mod(KeyCode::Char('r'), KeyModifiers::CONTROL)).await;
    // The Ctrl guard (`!m.contains(CONTROL)`) must keep Ctrl-r out of the plain-r
    // arm, and the Ctrl-r arm must run the refresh fan-out.
    assert_eq!(
        app.client_state.read().await.settings_state.repeat_mode,
        before,
        "Ctrl-r must not cycle repeat"
    );
    assert!(
        client
            .sent()
            .iter()
            .any(|r| matches!(r, DaemonRequest::RefreshArtists)),
        "Ctrl-r must trigger the refresh"
    );
}

#[tokio::test]
#[serial]
async fn f1_from_another_page_switches_to_library() {
    let client = RecordingClient::new();
    let mut app = app_with(client.clone());
    app.client_state.write().await.page = ferrosonic::app::state::Page::Queue;
    press(&mut app, key(KeyCode::F(1))).await;
    assert_eq!(
        app.client_state.read().await.page,
        ferrosonic::app::state::Page::Library,
        "F1 must switch to the Library page"
    );
}

#[tokio::test]
#[serial]
async fn fkey_clears_queue_naming_overlay() {
    let client = RecordingClient::new();
    let mut app = app_with(client.clone());
    {
        let mut cs = app.client_state.write().await;
        cs.page = ferrosonic::app::state::Page::Queue;
        cs.queue_state.naming_playlist = true;
    }
    press(&mut app, key(KeyCode::F(2))).await;
    assert!(
        !app.client_state.read().await.queue_state.naming_playlist,
        "an F-key must cancel the queue naming overlay"
    );
}

#[tokio::test]
#[serial]
async fn fkey_clears_playlist_edit_overlay() {
    let client = RecordingClient::new();
    let mut app = app_with(client.clone());
    {
        let mut cs = app.client_state.write().await;
        cs.page = ferrosonic::app::state::Page::Playlists;
        cs.playlists.renaming = true;
        cs.playlists.rename_buf = "x".into();
    }
    press(&mut app, key(KeyCode::F(1))).await;
    assert!(
        !app.client_state.read().await.playlists.renaming,
        "an F-key must cancel the playlist rename overlay"
    );
}

#[tokio::test]
#[serial]
async fn typing_routes_to_playlist_handler_while_renaming() {
    let client = RecordingClient::new();
    let mut app = app_with(client.clone());
    {
        let mut cs = app.client_state.write().await;
        cs.page = ferrosonic::app::state::Page::Playlists;
        cs.playlists.renaming = true;
        cs.playlists.confirming_delete = false;
    }
    // 'l' is the global Next binding; while renaming it must type, not fire Next.
    // The `||`->`&&` mutant skips the route, so 'l' would hit the global match.
    press(&mut app, key(KeyCode::Char('l'))).await;
    assert_eq!(
        app.client_state.read().await.playlists.rename_buf,
        "l",
        "a global-key char while renaming must type into the rename buffer"
    );
    assert!(
        !client
            .sent()
            .iter()
            .any(|r| matches!(r, DaemonRequest::Next)),
        "'l' while renaming must not fire Next"
    );
}

// Closes the handle_event Mouse-arm seam (delete arm): a mouse event must reach
// handle_mouse. Routed via handle_event (not handle_mouse) to exercise the wiring.
#[tokio::test]
#[serial]
async fn mouse_event_reaches_handle_mouse_via_handle_event() {
    use crossterm::event::{Event, MouseEvent, MouseEventKind};
    let client = RecordingClient::new();
    let mut app = app_with(client.clone());
    {
        let mut ds = app.daemon_state.write().await;
        ds.queue = vec![song("a"), song("b")];
    }
    app.client_state.write().await.page = ferrosonic::app::state::Page::Queue;
    let scroll = Event::Mouse(MouseEvent {
        kind: MouseEventKind::ScrollDown,
        column: 0,
        row: 0,
        modifiers: KeyModifiers::NONE,
    });
    app.handle_event(scroll).await.unwrap();
    assert_eq!(
        app.client_state.read().await.queue_state.selected,
        Some(0),
        "a scroll mouse event must be dispatched to handle_mouse"
    );
}

// Closes the 126:17 `&&`->`||` seam: on Playlists with no edit overlay, a global
// key must still fire globally, not get captured by the text-field route.
#[tokio::test]
#[serial]
async fn l_fires_next_on_playlists_when_not_editing() {
    let client = RecordingClient::new();
    let mut app = app_with(client.clone());
    {
        let mut cs = app.client_state.write().await;
        cs.page = ferrosonic::app::state::Page::Playlists;
        cs.playlists.renaming = false;
        cs.playlists.confirming_delete = false;
    }
    press(&mut app, key(KeyCode::Char('l'))).await;
    assert!(
        client
            .sent()
            .iter()
            .any(|r| matches!(r, DaemonRequest::Next)),
        "'l' on Playlists (not editing) must fire global Next, not route to the page handler"
    );
}
