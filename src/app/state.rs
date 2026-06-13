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

/// Top-level TUI page selected via the header tabs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Page {
    /// Artist tree with albums and songs.
    #[default]
    Library,
    /// Current play queue.
    Queue,
    /// Starred and random song lists.
    QuickPlay,
    /// Server-side playlists.
    Playlists,
    /// Server credentials editor.
    Server,
    /// Theme, cava, and playback settings.
    Settings,
}

impl Page {
    /// Zero-based tab position in the header.
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

    /// Tab caption shown in the header.
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

    /// Function key bound to this page.
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

/// Transient footer notification.
#[derive(Debug, Clone)]
pub struct Notification {
    /// Notification text.
    pub message: String,
    /// Whether to style the notification as an error.
    pub is_error: bool,
    /// Creation time, used for expiry.
    pub created_at: Instant,
}

/// Screen regions computed by the layout pass, reused for mouse hit-testing.
#[derive(Debug, Clone, Default)]
pub struct LayoutAreas {
    /// Header bar with tabs and transport buttons.
    pub header: Rect,
    /// Main content region between header and now-playing.
    pub content: Rect,
    /// Now-playing strip at the bottom.
    pub now_playing: Rect,
    /// Left pane of a split content region, when split.
    pub content_left: Option<Rect>,
    /// Right pane of a split content region, when split.
    pub content_right: Option<Rect>,
}

/// Render-pass borrow bundle. `daemon` is shared-read; `client` is exclusive for the scroll-offset + layout-rect writes.
pub struct AppState<'a> {
    /// Daemon state mirror, read-only during render.
    pub daemon: &'a crate::daemon::DaemonState,
    /// Client-local UI state, mutated during render.
    pub client: &'a mut crate::app::client_state::ClientState,
}

/// One rendered row of cava visualizer output.
#[derive(Debug, Clone, Default)]
pub struct CavaRow {
    /// Styled text runs making up the row.
    pub spans: Vec<CavaSpan>,
}

/// One styled text run within a cava row.
#[derive(Debug, Clone)]
pub struct CavaSpan {
    /// Run text.
    pub text: String,
    /// Foreground color.
    pub fg: CavaColor,
    /// Background color.
    pub bg: CavaColor,
}

/// Terminal color parsed from cava's ANSI output.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum CavaColor {
    /// Terminal default color.
    #[default]
    Default,
    /// 256-color palette index.
    Indexed(u8),
    /// Truecolor RGB value.
    Rgb(u8, u8, u8),
}

impl<'a> AppState<'a> {
    /// Song currently loaded in the daemon, if any.
    pub fn current_song(&self) -> Option<&Child> {
        self.daemon.current_song()
    }

    /// Song list backing the Quick Play page's active selection.
    pub fn songs_list(&self) -> &[Child] {
        match self.client.songs.selected_option {
            Some(SongOption::Random) => &self.daemon.library.random_songs,
            _ => &self.daemon.library.starred_songs,
        }
    }
}

/// Shared handle to the daemon state mirror.
pub type SharedDaemonState = Arc<RwLock<crate::daemon::DaemonState>>;
/// Shared handle to the client-local UI state.
pub type SharedClientState = Arc<RwLock<crate::app::client_state::ClientState>>;

/// Wrap a fresh `DaemonState` for sharing across tasks.
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

/// Build the client UI state pre-seeded from the persisted config.
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
