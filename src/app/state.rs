//! Shared application state.

use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;

use ratatui::layout::Rect;
use serde::{Deserialize, Serialize};

use crate::app::models::SongOption;
use crate::config::Config;
use crate::subsonic::models::Child;
use crate::ui::theme::{ThemeColors, ThemeData};

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum PlaybackState {
    #[default]
    Stopped,
    Playing,
    Paused,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NowPlaying {
    pub song: Option<Child>,
    pub state: PlaybackState,
    pub position: f64,
    pub duration: f64,
    pub sample_rate: Option<u32>,
    pub bit_depth: Option<u32>,
    pub format: Option<String>,
    /// "Stereo", "Mono", "5.1ch", etc.
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
    /// Song lists live in `daemon.library.starred_songs` /
    /// `random_songs`; resolve via `AppState::songs_list()`.
    pub selected_option: Option<SongOption>,
    pub selected_index: Option<usize>,
    pub focus: usize,
    pub scroll_offset: usize,
}

#[derive(Debug, Clone, Default)]
pub struct ArtistsState {
    pub selected_index: Option<usize>,
    pub expanded: std::collections::HashSet<String>,
    pub songs: Vec<Child>,
    pub selected_song: Option<usize>,
    pub filter: String,
    pub filter_active: bool,
    pub filter_scope: FilterScope,
    pub search_results: Option<crate::subsonic::models::SearchResult3>,
    /// Bumped on every keystroke; spawned search tasks only commit a
    /// reply if the gen still matches — drops stale results.
    pub search_gen: u64,
    /// 0 = tree, 1 = songs.
    pub focus: usize,
    pub tree_scroll_offset: usize,
    pub song_scroll_offset: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FilterScope {
    #[default]
    Artists,
    Albums,
    Songs,
}

impl FilterScope {
    pub fn label(self) -> &'static str {
        match self {
            FilterScope::Artists => "artists",
            FilterScope::Albums => "albums",
            FilterScope::Songs => "songs",
        }
    }
    pub fn cycle(self) -> Self {
        match self {
            FilterScope::Artists => FilterScope::Albums,
            FilterScope::Albums => FilterScope::Songs,
            FilterScope::Songs => FilterScope::Artists,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct QueueState {
    pub selected: Option<usize>,
    pub scroll_offset: usize,
}

#[derive(Debug, Clone, Default)]
pub struct PlaylistsState {
    pub selected_playlist: Option<usize>,
    pub songs: Vec<Child>,
    pub selected_song: Option<usize>,
    /// 0 = playlists, 1 = songs.
    pub focus: usize,
    pub playlist_scroll_offset: usize,
    pub song_scroll_offset: usize,
}

#[derive(Clone, Default)]
pub struct ServerState {
    /// 0=URL, 1=Username, 2=Password, 3=Test, 4=Save.
    pub selected_field: usize,
    pub base_url: String,
    pub username: String,
    pub password: String,
    pub status: Option<String>,
}

impl std::fmt::Debug for ServerState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServerState")
            .field("selected_field", &self.selected_field)
            .field("base_url", &self.base_url)
            .field("username", &self.username)
            .field(
                "password",
                &if self.password.is_empty() { "" } else { "***" },
            )
            .field("status", &self.status)
            .finish()
    }
}

#[derive(Debug, Clone)]
pub struct SettingsState {
    pub selected_field: usize,
    pub themes: Vec<ThemeData>,
    pub theme_index: usize,
    pub cava_enabled: bool,
    pub cava_size: u8,
    /// Takes effect on next TUI launch.
    pub daemon_enabled: bool,
    /// Auto-continue with random songs when the queue ends. Daemon
    /// fetches a fresh batch and keeps playing.
    pub auto_continue: bool,
    /// Repeat mode for the queue. Cycled by `r` globally.
    pub repeat_mode: crate::config::RepeatMode,
    /// Show cover art in the now-playing section.
    pub cover_art: bool,
    /// Total now-playing height (rows) when art is visible.
    pub cover_art_size: u8,
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
            auto_continue: false,
            repeat_mode: crate::config::RepeatMode::Off,
            cover_art: false,
            cover_art_size: 16,
        }
    }
}

impl SettingsState {
    pub fn theme_name(&self) -> &str {
        &self.themes[self.theme_index].name
    }

    pub fn theme_colors(&self) -> &ThemeColors {
        &self.themes[self.theme_index].colors
    }

    pub fn current_theme(&self) -> &ThemeData {
        &self.themes[self.theme_index]
    }

    pub fn next_theme(&mut self) {
        self.theme_index = (self.theme_index + 1) % self.themes.len();
    }

    pub fn prev_theme(&mut self) {
        self.theme_index = (self.theme_index + self.themes.len() - 1) % self.themes.len();
    }

    /// Returns true if `name` matched. Otherwise falls back to index 0.
    pub fn set_theme_by_name(&mut self, name: &str) -> bool {
        if let Some(idx) = self
            .themes
            .iter()
            .position(|t| t.name.eq_ignore_ascii_case(name))
        {
            self.theme_index = idx;
            true
        } else {
            self.theme_index = 0;
            false
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

/// Render-pass borrow bundle. `daemon` is shared-read; `client` is
/// exclusive for the scroll-offset + layout-rect writes.
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
