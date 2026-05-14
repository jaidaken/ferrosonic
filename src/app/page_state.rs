//! Per-page UI state structs and the FilterScope enum.

use crate::app::models::SongOption;
use crate::secret::Secret;
use crate::subsonic::models::Child;
use crate::ui::theme::{ThemeColors, ThemeData};

#[derive(Debug, Clone, Default)]
pub struct SongsState {
    /// Song lists live in `daemon.library.starred_songs` / `random_songs`; resolve via `AppState::songs_list()`.
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
    /// Bumped on every keystroke; spawned search tasks only commit a reply if the gen still matches, drops stale results.
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

#[derive(Clone, Default, Debug)]
pub struct ServerState {
    /// 0=URL, 1=Username, 2=Password, 3=Test, 4=Save.
    pub selected_field: usize,
    pub base_url: String,
    pub username: String,
    pub password: Secret,
    pub status: Option<String>,
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
