//! Shared application state.

use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;

use ratatui::layout::Rect;

use crate::app::models::SongOption;
use crate::config::Config;
use crate::subsonic::models::Child;

pub use crate::app::page_state::{
    ArtistsState, FilterScope, PlaylistsState, QueueState, ServerState, SettingsState, SongsState,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Page {
    #[default]
    Library,
    Queue,
    QuickPlay,
    Playlists,
    Server,
    Settings,
}

impl Page {
    pub fn index(&self) -> usize {
        match self {
            Page::Library => 0,
            Page::Queue => 1,
            Page::QuickPlay => 2,
            Page::Playlists => 3,
            Page::Server => 4,
            Page::Settings => 5,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Page::Library => "Library",
            Page::Queue => "Queue",
            Page::QuickPlay => "Quick Play",
            Page::Playlists => "Playlists",
            Page::Server => "Server",
            Page::Settings => "Settings",
        }
    }

    pub fn shortcut(&self) -> &'static str {
        match self {
            Page::Library => "F1",
            Page::Queue => "F2",
            Page::QuickPlay => "F3",
            Page::Playlists => "F4",
            Page::Server => "F5",
            Page::Settings => "F6",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Notification {
    pub message: String,
    pub is_error: bool,
    pub created_at: Instant,
}

#[derive(Debug, Clone, Default)]
pub struct LayoutAreas {
    pub header: Rect,
    pub content: Rect,
    pub now_playing: Rect,
    pub content_left: Option<Rect>,
    pub content_right: Option<Rect>,
}

/// Render-pass borrow bundle. `daemon` is shared-read; `client` is exclusive for the scroll-offset + layout-rect writes.
pub struct AppState<'a> {
    pub daemon: &'a crate::daemon::DaemonState,
    pub client: &'a mut crate::app::client_state::ClientState,
}

#[derive(Debug, Clone, Default)]
pub struct CavaRow {
    pub spans: Vec<CavaSpan>,
}

#[derive(Debug, Clone)]
pub struct CavaSpan {
    pub text: String,
    pub fg: CavaColor,
    pub bg: CavaColor,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum CavaColor {
    #[default]
    Default,
    Indexed(u8),
    Rgb(u8, u8, u8),
}

impl<'a> AppState<'a> {
    pub fn current_song(&self) -> Option<&Child> {
        self.daemon.current_song()
    }

    pub fn songs_list(&self) -> &[Child] {
        match self.client.songs.selected_option {
            Some(SongOption::Random) => &self.daemon.library.random_songs,
            _ => &self.daemon.library.starred_songs,
        }
    }
}

pub type SharedDaemonState = Arc<RwLock<crate::daemon::DaemonState>>;
pub type SharedClientState = Arc<RwLock<crate::app::client_state::ClientState>>;

pub fn new_shared_daemon_state(config: Config) -> SharedDaemonState {
    Arc::new(RwLock::new(crate::daemon::DaemonState::new(config)))
}

/// Daemon-only constructor: loads the persisted queue snapshot synchronously into the bare DaemonState before wrapping in Arc<RwLock>. Tests use new_shared_daemon_state to avoid reading the user's real ~/.config/ferrosonic/queue.json.
pub fn new_shared_daemon_state_with_restored_queue(config: Config) -> SharedDaemonState {
    let mut state = crate::daemon::DaemonState::new(config);
    if let Some(snap) = crate::daemon::persistence::QueueSnapshot::load() {
        let count = snap.queue.len();
        let pos = snap.position;
        state.queue = snap.queue;
        state.queue_position = pos;
        tracing::info!("Restored {} queue items (position={:?})", count, pos);
    }
    Arc::new(RwLock::new(state))
}

pub fn new_shared_client_state(config: &Config) -> SharedClientState {
    let mut client = crate::app::client_state::ClientState::default();
    client.server_state.base_url = config.base_url.clone();
    client.server_state.username = config.username.clone();
    client.server_state.password = config.password.clone();
    client.settings_state.cava_enabled = config.cava;
    client.settings_state.cava_size = config.cava_size.clamp(10, 80);
    client.settings_state.daemon_enabled = config.daemon;
    client.settings_state.auto_continue = config.auto_continue;
    client.settings_state.repeat_mode = config.repeat_mode;
    client.settings_state.cover_art = config.cover_art;
    client.settings_state.cover_art_size = config.cover_art_size.clamp(8, 24);
    client.songs.selected_option = Some(SongOption::Starred);
    Arc::new(RwLock::new(client))
}
