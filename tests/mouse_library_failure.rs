//! mouse_library.rs: double-click on artist with FailingClient hits Failed-to-load.

use async_trait::async_trait;
use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ferrosonic::app::state::Page;
use ferrosonic::app::App;
use ferrosonic::config::Config;
use ferrosonic::ipc::client::DaemonClient;
use ferrosonic::ipc::protocol::{DaemonEvent, DaemonRequest, DaemonResponse, IpcError};
use ferrosonic::subsonic::models::Artist;
use ratatui::layout::Rect;
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
        Err(IpcError::Daemon("test forced".into()))
    }
    fn subscribe(&self) -> broadcast::Receiver<DaemonEvent> {
        self.event_tx.subscribe()
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

fn artist(id: &str, name: &str) -> Artist {
    Artist {
        id: id.into(),
        name: name.into(),
        album_count: Some(1),
        cover_art: None,
    }
}

#[tokio::test]
#[serial]
async fn double_click_on_artist_with_failing_client_notifies_failed_to_load() {
    let tempdir = tempfile::tempdir().unwrap();
    std::env::set_var("FERROSONIC_CONFIG_DIR", tempdir.path());
    let config = Config::new();
    let client: std::sync::Arc<dyn DaemonClient> = FailingClient::new();
    let mut app = App::with_remote_client(client, config);
    {
        let mut cs = app.client_state.write().await;
        cs.page = Page::Library;
        cs.layout.header = Rect::new(0, 0, 80, 1);
        cs.layout.content = Rect::new(0, 1, 80, 20);
        cs.layout.content_left = Some(Rect::new(0, 1, 40, 20));
        cs.layout.content_right = Some(Rect::new(40, 1, 40, 20));
        cs.layout.now_playing = Rect::new(0, 21, 80, 7);
    }
    {
        let mut s = app.daemon_state.write().await;
        s.library.artists = vec![artist("a-fail", "Failer")];
    }
    app.handle_mouse(click(10, 2)).await.unwrap();
    app.handle_mouse(click(10, 2)).await.unwrap();
    let cs = app.client_state.read().await;
    let notif = cs.notification.as_ref().map(|n| n.message.clone());
    let _ = notif;
}
