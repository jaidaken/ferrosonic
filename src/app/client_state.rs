//! Client-side state. Everything the TUI owns and mutates locally: which
//! page is shown, per-page selection/scroll/focus, theme/cava UI preferences,
//! transient notifications, layout cache for mouse hit-testing, and the cava
//! visualizer screen buffer.
//!
//! Phase 1 introduces this type alongside `DaemonState`; both are embedded
//! in `AppState` for now. Phase 5 splits the daemon into its own process and
//! `ClientState` lives in the TUI binary, never crossing the socket.

use std::time::Instant;

use crate::app::state::{
    ArtistsState, CavaRow, LayoutAreas, Notification, Page, PlaylistsState, QueueState,
    ServerState, SettingsState, SongsState,
};

/// State owned exclusively by the TUI client. Page selection, per-page UI
/// state (selection indices, scroll offsets, focus, filter input), theme
/// data, notifications, cava visualizer buffer, and the layout cache used
/// for mouse hit-testing.
#[derive(Debug, Default)]
pub struct ClientState {
    /// Currently displayed page.
    pub page: Page,
    /// Songs page selection state (which row is selected, which option,
    /// where the list is scrolled). The actual song list lives in the
    /// daemon's `LibraryCache`.
    pub songs: SongsState,
    /// Artists page selection/expansion state. Artist list and album
    /// cache live in the daemon.
    pub artists: ArtistsState,
    /// Queue page selection + scroll. Queue contents live in the daemon.
    pub queue_state: QueueState,
    /// Playlists page selection state. Playlist list and per-playlist
    /// songs live in the daemon.
    pub playlists: PlaylistsState,
    /// Server settings page (URL/username/password being edited).
    pub server_state: ServerState,
    /// Settings page (theme + cava knobs).
    pub settings_state: SettingsState,
    /// Current toast/notification.
    pub notification: Option<Notification>,
    /// Whether the TUI should exit on next tick.
    pub should_quit: bool,
    /// Cava visualizer screen buffer (rendered output rows).
    pub cava_screen: Vec<CavaRow>,
    /// Whether the cava binary is available on the system.
    pub cava_available: bool,
    /// Cached layout areas from last render — used for mouse hit-testing.
    pub layout: LayoutAreas,
}

impl ClientState {
    /// Show a non-error notification.
    pub fn notify(&mut self, message: impl Into<String>) {
        self.notification = Some(Notification {
            message: message.into(),
            is_error: false,
            created_at: Instant::now(),
        });
    }

    /// Show an error notification.
    pub fn notify_error(&mut self, message: impl Into<String>) {
        self.notification = Some(Notification {
            message: message.into(),
            is_error: true,
            created_at: Instant::now(),
        });
    }

    /// Clear the notification if older than 2 seconds. Called every tick.
    pub fn check_notification_timeout(&mut self) {
        if let Some(ref notif) = self.notification {
            if notif.created_at.elapsed().as_secs() >= 2 {
                self.notification = None;
            }
        }
    }

    /// Clear the notification immediately.
    pub fn clear_notification(&mut self) {
        self.notification = None;
    }
}
