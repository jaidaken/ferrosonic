//! TUI-only state: page, per-page UI, theme prefs, cava buffer, toasts.

use std::time::Instant;

use crate::app::state::{
    ArtistsState, CavaRow, LayoutAreas, Notification, Page, PlaylistsState, QueueState,
    ServerState, SettingsState, SongsState,
};

#[derive(Debug, Default)]
pub struct ClientState {
    pub page: Page,
    pub songs: SongsState,
    pub artists: ArtistsState,
    pub queue_state: QueueState,
    pub playlists: PlaylistsState,
    pub server_state: ServerState,
    pub settings_state: SettingsState,
    pub notification: Option<Notification>,
    pub should_quit: bool,
    pub cava_screen: Vec<CavaRow>,
    pub cava_available: bool,
    pub layout: LayoutAreas,
}

impl ClientState {
    pub fn notify(&mut self, message: impl Into<String>) {
        self.notification = Some(Notification {
            message: message.into(),
            is_error: false,
            created_at: Instant::now(),
        });
    }

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

    pub fn clear_notification(&mut self) {
        self.notification = None;
    }
}
