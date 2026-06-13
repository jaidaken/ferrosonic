//! TUI-only state: page, per-page UI, theme prefs, cava buffer, toasts.

use std::time::Instant;

use crate::app::state::{
    ArtistsState, CavaRow, LayoutAreas, Notification, Page, PlaylistsState, QueueState,
    ServerState, SettingsState, SongsState,
};

/// All client-local UI state; never leaves the TUI process.
#[derive(Debug, Default)]
pub struct ClientState {
    /// Currently displayed page.
    pub page: Page,
    /// Quick Play page state.
    pub songs: SongsState,
    /// Library page state.
    pub artists: ArtistsState,
    /// Queue page state.
    pub queue_state: QueueState,
    /// Playlists page state.
    pub playlists: PlaylistsState,
    /// Server credentials page state.
    pub server_state: ServerState,
    /// Settings page state.
    pub settings_state: SettingsState,
    /// Active footer notification, if any.
    pub notification: Option<Notification>,
    /// Set to end the main event loop.
    pub should_quit: bool,
    /// Latest parsed cava frame, one entry per row.
    pub cava_screen: Vec<CavaRow>,
    /// Whether a cava binary was found on PATH.
    pub cava_available: bool,
    /// Screen regions from the last layout pass.
    pub layout: LayoutAreas,
}

impl ClientState {
    /// Show an informational footer notification.
    pub fn notify(&mut self, message: impl Into<String>) {
        self.notification = Some(Notification {
            message: message.into(),
            is_error: false,
            created_at: Instant::now(),
        });
    }

    /// Show an error-styled footer notification.
    pub fn notify_error(&mut self, message: impl Into<String>) {
        self.notification = Some(Notification {
            message: message.into(),
            is_error: true,
            created_at: Instant::now(),
        });
    }

    /// Called every tick; clears notifications older than 2s.
    pub fn check_notification_timeout(&mut self) {
        if let Some(ref notif) = self.notification {
            if notif.created_at.elapsed().as_secs() >= 2 {
                self.notification = None;
            }
        }
    }

    /// Dismiss the current notification immediately.
    pub fn clear_notification(&mut self) {
        self.notification = None;
    }
}
