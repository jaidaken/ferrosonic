//! Per-page UI state structs and the FilterScope enum.

use crate::app::models::SongOption;
use crate::secret::Secret;
use crate::subsonic::models::Child;
use crate::ui::theme::{ThemeColors, ThemeData};

/// UI state of the Songs page.
#[derive(Debug, Clone, Default)]
pub struct SongsState {
    /// Song lists live in `daemon.library.starred_songs` / `random_songs`; resolve via `AppState::songs_list()`.
    pub selected_option: Option<SongOption>,
    /// Highlighted song index within the active list.
    pub selected_index: Option<usize>,
    /// Focused pane: 0 = list selector, 1 = songs.
    pub focus: usize,
    /// First visible row of the song list.
    pub scroll_offset: usize,
}

/// UI state of the Library (artist tree) page.
#[derive(Debug, Clone, Default)]
pub struct ArtistsState {
    /// Highlighted row in the artist tree.
    pub selected_index: Option<usize>,
    /// IDs of artists whose album list is expanded.
    pub expanded: std::collections::HashSet<String>,
    /// Songs shown in the right-hand pane.
    pub songs: Vec<Child>,
    /// Highlighted song index in the right-hand pane.
    pub selected_song: Option<usize>,
    /// Current filter text.
    pub filter: String,
    /// Whether the filter input is capturing keystrokes.
    pub filter_active: bool,
    /// Which item kind the filter matches against.
    pub filter_scope: FilterScope,
    /// Server-side search results replacing the tree while filtering.
    pub search_results: Option<crate::subsonic::models::SearchResult3>,
    /// Bumped on every keystroke; spawned search tasks only commit a reply if the gen still matches, drops stale results.
    pub search_gen: u64,
    /// 0 = tree, 1 = songs.
    pub focus: usize,
    /// First visible row of the artist tree.
    pub tree_scroll_offset: usize,
    /// First visible row of the song pane.
    pub song_scroll_offset: usize,
}

/// Item kind the Library page filter matches against.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FilterScope {
    /// Match artist names.
    #[default]
    Artists,
    /// Match album titles.
    Albums,
    /// Match song titles.
    Songs,
}

impl FilterScope {
    /// Lowercase label shown in the filter prompt.
    pub fn label(self) -> &'static str {
        match self {
            FilterScope::Artists => "artists",
            FilterScope::Albums => "albums",
            FilterScope::Songs => "songs",
        }
    }
    /// Next scope in the artists, albums, songs rotation.
    pub fn cycle(self) -> Self {
        match self {
            FilterScope::Artists => FilterScope::Albums,
            FilterScope::Albums => FilterScope::Songs,
            FilterScope::Songs => FilterScope::Artists,
        }
    }
}

/// UI state of the Queue page.
#[derive(Debug, Clone, Default)]
pub struct QueueState {
    /// Highlighted queue entry.
    pub selected: Option<usize>,
    /// First visible row of the queue list.
    pub scroll_offset: usize,
}

/// UI state of the Playlists page.
#[derive(Debug, Clone, Default)]
pub struct PlaylistsState {
    /// Highlighted playlist index.
    pub selected_playlist: Option<usize>,
    /// Songs of the selected playlist.
    pub songs: Vec<Child>,
    /// Highlighted song index in the song pane.
    pub selected_song: Option<usize>,
    /// 0 = playlists, 1 = songs.
    pub focus: usize,
    /// First visible row of the playlist list.
    pub playlist_scroll_offset: usize,
    /// First visible row of the song pane.
    pub song_scroll_offset: usize,
}

/// UI state of the Server (credentials) page.
#[derive(Clone, Default, Debug)]
pub struct ServerState {
    /// 0=URL, 1=Username, 2=Password, 3=Test, 4=Save.
    pub selected_field: usize,
    /// Server base URL being edited.
    pub base_url: String,
    /// Username being edited.
    pub username: String,
    /// Password being edited.
    pub password: Secret,
    /// Status line from the last test or save action.
    pub status: Option<String>,
}

/// UI state of the Settings page.
#[derive(Debug, Clone)]
pub struct SettingsState {
    /// Index of the focused settings row.
    pub selected_field: usize,
    /// All themes available for selection.
    pub themes: Vec<ThemeData>,
    /// Index of the active theme in `themes`.
    pub theme_index: usize,
    /// Whether the cava visualizer is enabled.
    pub cava_enabled: bool,
    /// Cava visualizer height in rows.
    pub cava_size: u8,
    /// Takes effect on next TUI launch.
    pub daemon_enabled: bool,
    /// Auto-continue with random songs when the queue ends. Daemon fetches a fresh batch and keeps playing.
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
    /// Name of the active theme.
    pub fn theme_name(&self) -> &str {
        &self.themes[self.theme_index].name
    }

    /// Color palette of the active theme.
    pub fn theme_colors(&self) -> &ThemeColors {
        &self.themes[self.theme_index].colors
    }

    /// The active theme.
    pub fn current_theme(&self) -> &ThemeData {
        &self.themes[self.theme_index]
    }

    /// Advance to the next theme, wrapping at the end.
    pub fn next_theme(&mut self) {
        self.theme_index = (self.theme_index + 1) % self.themes.len();
    }

    /// Step back to the previous theme, wrapping at the start.
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
