//! Shared application state

use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;

use ratatui::layout::Rect;
use serde::{Deserialize, Serialize};

use crate::app::models::SongOption;
use crate::config::Config;
use crate::subsonic::models::Child;
use crate::ui::theme::{ThemeColors, ThemeData};

// `Notification`, `LayoutAreas`, etc. live here but are read from
// `client_state::ClientState`. Keep them re-exported so existing imports
// (`use crate::app::state::Notification`) continue to compile.

/// Current page in the application
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

/// Playback state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum PlaybackState {
    #[default]
    Stopped,
    Playing,
    Paused,
}

/// Now playing information
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NowPlaying {
    /// Currently playing song
    pub song: Option<Child>,
    /// Playback state
    pub state: PlaybackState,
    /// Current position in seconds
    pub position: f64,
    /// Total duration in seconds
    pub duration: f64,
    /// Audio sample rate (Hz)
    pub sample_rate: Option<u32>,
    /// Audio bit depth
    pub bit_depth: Option<u32>,
    /// Audio format/codec
    pub format: Option<String>,
    /// Audio channel layout (e.g., "Stereo", "Mono", "5.1ch")
    pub channels: Option<String>,
}

impl NowPlaying {
    pub fn progress_percent(&self) -> f64 {
        if self.duration > 0.0 {
            (self.position / self.duration).clamp(0.0, 1.0)
        } else {
            0.0
        }
    }

    pub fn format_position(&self) -> String {
        format_duration(self.position)
    }

    pub fn format_duration(&self) -> String {
        format_duration(self.duration)
    }
}

/// Format duration in MM:SS or HH:MM:SS format
pub fn format_duration(seconds: f64) -> String {
    let total_secs = seconds as u64;
    let hours = total_secs / 3600;
    let mins = (total_secs % 3600) / 60;
    let secs = total_secs % 60;

    if hours > 0 {
        format!("{:02}:{:02}:{:02}", hours, mins, secs)
    } else {
        format!("{:02}:{:02}", mins, secs)
    }
}

#[derive(Debug, Clone, Default)]
pub struct SongsState {
    /// Which option (Starred / Random) is selected. The actual song list
    /// for this option lives in `daemon.library.starred_songs` /
    /// `random_songs`; use `AppState::songs_list()` to resolve.
    pub selected_option: Option<SongOption>,
    pub selected_index: Option<usize>,
    pub focus: usize,
    pub scroll_offset: usize,
}

/// Artists page state — pure UI state (selection, expansion, filter, focus,
/// scroll). The artist list and album cache live in `daemon.library`.
#[derive(Debug, Clone, Default)]
pub struct ArtistsState {
    /// Currently selected index in the tree (artists + expanded albums)
    pub selected_index: Option<usize>,
    /// Set of expanded artist IDs
    pub expanded: std::collections::HashSet<String>,
    /// Songs in the selected album (shown in right pane). Stays client-side
    /// in phase 1 (it's populated by the user's click action). Phase 2 lifts
    /// this into `daemon.library.album_songs_cache` keyed by album id.
    pub songs: Vec<Child>,
    /// Currently selected song index
    pub selected_song: Option<usize>,
    /// Artist filter text
    pub filter: String,
    /// Whether filter input is active
    pub filter_active: bool,
    /// Focus: 0 = tree, 1 = songs
    pub focus: usize,
    /// Scroll offset for the tree list (set after render)
    pub tree_scroll_offset: usize,
    /// Scroll offset for the songs list (set after render)
    pub song_scroll_offset: usize,
}

/// Queue page state
#[derive(Debug, Clone, Default)]
pub struct QueueState {
    /// Currently selected index in the queue
    pub selected: Option<usize>,
    /// Scroll offset for the queue list (set after render)
    pub scroll_offset: usize,
}

/// Playlists page state — pure UI state. The playlist list lives in
/// `daemon.library.playlists`.
#[derive(Debug, Clone, Default)]
pub struct PlaylistsState {
    /// Currently selected playlist index
    pub selected_playlist: Option<usize>,
    /// Songs in the selected playlist. Stays client-side in phase 1; phase 2
    /// lifts into `daemon.library.playlist_songs_cache` keyed by playlist id.
    pub songs: Vec<Child>,
    /// Currently selected song index
    pub selected_song: Option<usize>,
    /// Focus: 0 = playlists, 1 = songs
    pub focus: usize,
    /// Scroll offset for the playlists list (set after render)
    pub playlist_scroll_offset: usize,
    /// Scroll offset for the songs list (set after render)
    pub song_scroll_offset: usize,
}

/// Server page state (connection settings)
#[derive(Debug, Clone, Default)]
pub struct ServerState {
    /// Currently focused field (0-4: URL, Username, Password, Test, Save)
    pub selected_field: usize,
    /// Edit values
    pub base_url: String,
    pub username: String,
    pub password: String,
    /// Status message
    pub status: Option<String>,
}

/// Settings page state
#[derive(Debug, Clone)]
pub struct SettingsState {
    /// Currently focused field (0=Theme, 1=Cava, 2=CavaSize, 3=Daemon).
    pub selected_field: usize,
    /// Available themes (Default + loaded from files)
    pub themes: Vec<ThemeData>,
    /// Index of the currently selected theme in `themes`
    pub theme_index: usize,
    /// Cava visualizer enabled
    pub cava_enabled: bool,
    /// Cava visualizer height percentage (10-80, step 5)
    pub cava_size: u8,
    /// Daemon mode enabled. When `true`, `ferrosonic` auto-spawns and
    /// connects to `ferrosonicd` (music persists across TUI close).
    /// When `false`, runs standalone. Takes effect on next launch.
    pub daemon_enabled: bool,
}

impl Default for SettingsState {
    fn default() -> Self {
        Self {
            selected_field: 0,
            themes: vec![ThemeData::default_theme()],
            theme_index: 0,
            cava_enabled: false,
            cava_size: 40,
            daemon_enabled: true,
        }
    }
}

impl SettingsState {
    /// Current theme name
    pub fn theme_name(&self) -> &str {
        &self.themes[self.theme_index].name
    }

    /// Current theme colors
    pub fn theme_colors(&self) -> &ThemeColors {
        &self.themes[self.theme_index].colors
    }

    /// Current theme data
    pub fn current_theme(&self) -> &ThemeData {
        &self.themes[self.theme_index]
    }

    /// Cycle to next theme
    pub fn next_theme(&mut self) {
        self.theme_index = (self.theme_index + 1) % self.themes.len();
    }

    /// Cycle to previous theme
    pub fn prev_theme(&mut self) {
        self.theme_index = (self.theme_index + self.themes.len() - 1) % self.themes.len();
    }

    /// Set theme by name, returning true if found
    pub fn set_theme_by_name(&mut self, name: &str) -> bool {
        if let Some(idx) = self
            .themes
            .iter()
            .position(|t| t.name.eq_ignore_ascii_case(name))
        {
            self.theme_index = idx;
            true
        } else {
            self.theme_index = 0; // Fall back to Default
            false
        }
    }
}

/// Notification/alert to display
#[derive(Debug, Clone)]
pub struct Notification {
    pub message: String,
    pub is_error: bool,
    pub created_at: Instant,
}

/// Cached layout rectangles from the last render, used for mouse hit-testing.
/// Automatically updated every frame, so resize and visualiser toggle are handled.
#[derive(Debug, Clone, Default)]
pub struct LayoutAreas {
    pub header: Rect,
    pub content: Rect,
    pub now_playing: Rect,
    /// Left pane for dual-pane pages (Artists tree, Playlists list)
    pub content_left: Option<Rect>,
    /// Right pane for dual-pane pages (Songs list)
    pub content_right: Option<Rect>,
}

/// Borrow-bundle for the render pass. Holds a shared reference to
/// `DaemonState` (read-only at render time) and an exclusive reference
/// to `ClientState` (renderers write scroll offsets + layout rects).
///
/// Each lock is held independently — see `SharedDaemonState` and
/// `SharedClientState`. The bundle exists so page renderers keep
/// using `state.daemon.X` / `state.client.X` field paths unchanged.
pub struct AppState<'a> {
    pub daemon: &'a crate::daemon::DaemonState,
    pub client: &'a mut crate::app::client_state::ClientState,
}

/// A row of styled segments from cava's terminal output
#[derive(Debug, Clone, Default)]
pub struct CavaRow {
    pub spans: Vec<CavaSpan>,
}

/// A styled text segment from cava's terminal output
#[derive(Debug, Clone)]
pub struct CavaSpan {
    pub text: String,
    pub fg: CavaColor,
    pub bg: CavaColor,
}

/// Color from cava's terminal output
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

/// Daemon-side state shared across the TUI's render path, the event
/// pump (which writes from server events), and `DaemonCore` (which
/// writes from audio + library operations). RwLock so MPRIS reads
/// don't block render reads.
pub type SharedDaemonState = Arc<RwLock<crate::daemon::DaemonState>>;

/// Client-side state shared between input handlers (writers) and the
/// render path (reads + scroll-offset writes). RwLock to allow MPRIS
/// reads + event-pump reads in parallel with render.
pub type SharedClientState = Arc<RwLock<crate::app::client_state::ClientState>>;

pub fn new_shared_daemon_state(config: Config) -> SharedDaemonState {
    Arc::new(RwLock::new(crate::daemon::DaemonState::new(config)))
}

pub fn new_shared_client_state(config: &Config) -> SharedClientState {
    let mut client = crate::app::client_state::ClientState::default();
    client.server_state.base_url = config.base_url.clone();
    client.server_state.username = config.username.clone();
    client.server_state.password = config.password.clone();
    client.settings_state.cava_enabled = config.cava;
    client.settings_state.cava_size = config.cava_size.clamp(10, 80);
    client.settings_state.daemon_enabled = config.daemon;
    // Songs-page default: Starred. The page's input handler treats
    // a None option as "do nothing" on arrow keys.
    client.songs.selected_option = Some(SongOption::Starred);
    Arc::new(RwLock::new(client))
}
