//! Per-page UI state structs.

use crate::app::models::SongOption;
use crate::secret::Secret;
use crate::subsonic::models::Child;
use crate::ui::theme::{ThemeColors, ThemeData};

/// Current result state for the lyrics overlay.
#[derive(Debug, Clone, Default)]
pub enum LyricsStatus {
    /// No request has been made for the current track.
    #[default]
    Idle,
    /// A background request is in flight.
    Loading,
    /// One or more lyric sources are available.
    Ready(Vec<crate::subsonic::models::LyricsSource>),
    /// The server returned no lyrics for the track.
    Empty,
    /// Retrieval failed; contains a user-facing reason.
    Error(String),
}

/// Client-owned lyrics overlay and per-song cache.
#[derive(Debug, Clone, Default)]
pub struct LyricsState {
    /// Whether the overlay is visible.
    pub open: bool,
    /// Song currently represented by `status`.
    pub song_id: Option<String>,
    /// Loading/result state for the displayed song.
    pub status: LyricsStatus,
    /// Successful results, including empty vectors, keyed by song ID.
    pub cache: std::collections::HashMap<String, Vec<crate::subsonic::models::LyricsSource>>,
    /// Selected language/source index.
    pub selected_source: usize,
    /// First lyric row displayed.
    pub scroll: usize,
    /// Follow the active synchronized line during playback.
    pub follow: bool,
    /// Guards against stale background replies replacing a newer track.
    pub request_generation: u64,
}

/// Which entity an info overlay describes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InfoKind {
    /// Artist biography/links.
    #[default]
    Artist,
    /// Album notes/links.
    Album,
}

/// Loaded payload for the info overlay.
#[derive(Debug, Clone)]
pub enum InfoPayload {
    /// Artist biography/links.
    Artist(crate::subsonic::models::ArtistInfo2),
    /// Album notes/links.
    Album(crate::subsonic::models::AlbumInfo),
}

/// Result state of the info overlay.
#[derive(Debug, Clone, Default)]
pub enum InfoStatus {
    /// No request has been made.
    #[default]
    Idle,
    /// A background request is in flight.
    Loading,
    /// Information is available.
    Ready(InfoPayload),
    /// The server returned no information (common without Last.fm).
    Empty,
    /// Retrieval failed; contains a user-facing reason.
    Error(String),
}

/// Client-owned artist/album info overlay.
#[derive(Debug, Clone, Default)]
pub struct InfoOverlayState {
    /// Whether the overlay is visible.
    pub open: bool,
    /// Artist or album.
    pub kind: InfoKind,
    /// ID of the described entity.
    pub target_id: Option<String>,
    /// Display name shown in the title.
    pub title: String,
    /// Loading/result state.
    pub status: InfoStatus,
    /// First visible body row.
    pub scroll: usize,
    /// Guards against stale background replies replacing a newer target.
    pub request_generation: u64,
}

/// Which playback-filter name list is being edited.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterListKind {
    /// Song genre exclusions.
    Genres,
    /// Song artist exclusions.
    Artists,
}

/// Staged playback-filter list edits. The daemon config is untouched until Save.
#[derive(Debug, Clone)]
pub struct FilterListEditor {
    /// Target list.
    pub kind: FilterListKind,
    /// Staged entries.
    pub entries: Vec<String>,
    /// Highlighted entry index.
    pub selected: usize,
    /// Whether the add-entry input owns keystrokes.
    pub adding: bool,
    /// New entry input buffer.
    pub input: String,
}

/// Staged global keybinding edits. The running keymap is untouched until Save.
#[derive(Debug, Clone)]
pub struct KeybindingEditor {
    /// Staged config overrides.
    pub bindings: std::collections::HashMap<
        crate::config::keybind::GlobalAction,
        crate::config::keybind::KeyChord,
    >,
    /// Highlighted action in `DEFAULT_BINDINGS` order.
    pub selected: usize,
    /// Whether the next key event should become the highlighted binding.
    pub capturing: bool,
}

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
    /// Server-side search results replacing the tree while filtering.
    pub search_results: Option<crate::subsonic::models::SearchResult3>,
    /// Bumped on every keystroke; spawned search tasks only commit a reply if the gen still matches, drops stale results.
    pub search_gen: u64,
    /// Recall cursor into `ClientState::search_history` while the filter is
    /// being edited; `None` means the live typed query, not a recalled one.
    pub history_cursor: Option<usize>,
    /// 0 = tree, 1 = songs.
    pub focus: usize,
    /// First visible row of the artist tree.
    pub tree_scroll_offset: usize,
    /// First visible row of the song pane.
    pub song_scroll_offset: usize,
    /// Left-pane view: artist tree (default) or flat album list.
    pub view: LibraryView,
    /// Sort order for the flat album list.
    pub album_sort: AlbumSort,
    /// Flat album list, pulled from the daemon on first switch to album view.
    pub albums: Vec<crate::subsonic::models::Album>,
    /// Highlighted album in the album-list view.
    pub album_selected: Option<usize>,
    /// First visible row of the album list.
    pub album_scroll_offset: usize,
}

impl ArtistsState {
    /// Reset the library search back to the pristine artist tree.
    ///
    /// ```
    /// use ferrosonic::app::page_state::ArtistsState;
    /// let mut a = ArtistsState::default();
    /// a.filter = "cure".into();
    /// a.filter_active = true;
    /// a.exit_search();
    /// assert!(a.filter.is_empty() && !a.filter_active);
    /// ```
    pub fn exit_search(&mut self) {
        self.filter_active = false;
        self.filter.clear();
        self.search_results = None;
        self.history_cursor = None;
        self.expanded.clear();
        self.selected_index = Some(0);
    }
}

/// Library left-pane view mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LibraryView {
    /// Artists with expandable albums (default).
    #[default]
    ArtistTree,
    /// Flat list of every album.
    AlbumList,
}

/// Sort order for the flat album list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AlbumSort {
    /// Alphabetical by album name.
    #[default]
    Name,
    /// By original release year, oldest first.
    ReleaseDate,
}

impl AlbumSort {
    /// The next sort in the cycle.
    #[must_use]
    pub const fn next(self) -> Self {
        match self {
            Self::Name => Self::ReleaseDate,
            Self::ReleaseDate => Self::Name,
        }
    }

    /// Short label for the pane title.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Name => "Name",
            Self::ReleaseDate => "Date",
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
    /// True while the save-as-playlist name box is capturing input.
    pub naming_playlist: bool,
    /// Playlist name being typed in that box.
    pub playlist_name: String,
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
    /// True while the rename box is capturing input.
    pub renaming: bool,
    /// New playlist name being typed in the rename box.
    pub rename_buf: String,
    /// True while the delete-confirmation prompt is showing.
    pub confirming_delete: bool,
}

/// Overlay state for adding a song to a playlist, opened from any song pane.
#[derive(Debug, Clone, Default)]
pub struct PlaylistPicker {
    /// True while the picker overlay is capturing input.
    pub active: bool,
    /// Highlighted playlist index in the picker list.
    pub selected: usize,
    /// The song queued to be added once a playlist is chosen.
    pub song: Option<Child>,
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
    /// Last locally committed/resolved password. Daemon snapshots are
    /// scrubbed, so this remains the safe source for reverting editor text.
    pub committed_password: Secret,
    /// Status line from the last test or save action.
    pub status: Option<String>,
}

/// UI state of the Settings page.
#[derive(Debug, Clone)]
// Independent settings toggles mirroring the config; orthogonal on/off flags.
#[allow(clippy::struct_excessive_bools)]
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
    /// Stream the next cold track immediately instead of pre-buffering it fully.
    pub stream_on_start: bool,
    /// Restore the queue, track, and playhead across daemon restarts.
    pub resume_on_start: bool,
    /// Auto-play a restored session instead of restoring it paused.
    pub autoplay_on_start: bool,
    /// Cache streamed tracks locally for repeat/offline playback.
    pub offline_cache_enabled: bool,
    /// Maximum offline cache size in MiB.
    pub offline_cache_max_mb: u32,
    /// Repeat mode for the queue. Cycled by `r` globally.
    pub repeat_mode: crate::config::RepeatMode,
    /// Show cover art in the now-playing section.
    pub cover_art: bool,
    /// Total now-playing height (rows) when art is visible.
    pub cover_art_size: u8,
    /// Report plays to the server (scrobble / playbackReport).
    pub scrobble: bool,
    /// Show a desktop notification on track change.
    pub notifications: bool,
    /// `ReplayGain` adjustment mode, pushed live to mpv.
    pub replay_gain_mode: crate::config::ReplayGainMode,
    /// `ReplayGain` preamp in dB (-15.0..=15.0), pushed live to mpv.
    pub replay_gain_preamp: f64,
    /// Prevent clipping from `ReplayGain` amplification, pushed live to mpv.
    pub replay_gain_clip: bool,
    /// Queue exclusion rules (rating/year/duration get TUI rows here;
    /// genre/artist exclude lists are `config.toml`-only for now).
    pub playback_filters: crate::config::PlaybackFilters,
    /// Persisted global keybinding overrides currently active in the TUI.
    pub keybindings: std::collections::HashMap<
        crate::config::keybind::GlobalAction,
        crate::config::keybind::KeyChord,
    >,
    /// Active excluded-genre/artist editor overlay.
    pub filter_editor: Option<FilterListEditor>,
    /// Active global-keybinding editor overlay.
    pub keybinding_editor: Option<KeybindingEditor>,
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
            stream_on_start: true,
            resume_on_start: true,
            autoplay_on_start: false,
            offline_cache_enabled: false,
            offline_cache_max_mb: 2048,
            repeat_mode: crate::config::RepeatMode::Off,
            cover_art: false,
            cover_art_size: 16,
            scrobble: true,
            notifications: true,
            replay_gain_mode: crate::config::ReplayGainMode::Off,
            replay_gain_preamp: 0.0,
            replay_gain_clip: false,
            playback_filters: crate::config::PlaybackFilters::default(),
            keybindings: std::collections::HashMap::new(),
            filter_editor: None,
            keybinding_editor: None,
        }
    }
}

impl SettingsState {
    /// Name of the active theme.
    #[must_use]
    pub fn theme_name(&self) -> &str {
        &self.themes[self.theme_index].name
    }

    /// Color palette of the active theme.
    #[must_use]
    pub fn theme_colors(&self) -> &ThemeColors {
        &self.themes[self.theme_index].colors
    }

    /// The active theme.
    #[must_use]
    pub fn current_theme(&self) -> &ThemeData {
        &self.themes[self.theme_index]
    }

    /// Advance to the next theme, wrapping at the end.
    pub const fn next_theme(&mut self) {
        self.theme_index = (self.theme_index + 1) % self.themes.len();
    }

    /// Step back to the previous theme, wrapping at the start.
    pub const fn prev_theme(&mut self) {
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
