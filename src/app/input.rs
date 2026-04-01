use crossterm::event::{self, Event, KeyCode, KeyModifiers};

use crate::error::Error;

use super::*;

impl App {
    /// Handle terminal events
    pub(super) async fn handle_event(&mut self, event: Event) -> Result<(), Error> {
        match event {
            Event::Key(key) => {
                // Only handle key press events, ignore release and repeat
                if key.kind == event::KeyEventKind::Press {
                    self.handle_key(key).await
                } else {
                    Ok(())
                }
            }
            Event::Mouse(mouse) => self.handle_mouse(mouse).await,
            Event::Resize(_, _) => {
                // Restart cava so it picks up the new terminal dimensions
                if self.cava_parser.is_some() {
                    let state = self.state.read().await;
                    let td = state.settings_state.current_theme();
                    let g = td.cava_gradient.clone();
                    let h = td.cava_horizontal_gradient.clone();
                    let cs = state.settings_state.cava_size as u32;
                    drop(state);
                    self.start_cava(&g, &h, cs);
                    let mut state = self.state.write().await;
                    state.cava_screen.clear();
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }

    /// Handle keyboard input
    pub(super) async fn handle_key(&mut self, key: event::KeyEvent) -> Result<(), Error> {
        let mut state = self.state.write().await;

        // Clear notification on any keypress
        state.clear_notification();

        // Bypass global keybindings when typing in server text fields or filtering artists
        let is_server_text_field =
            state.page == Page::Server && state.server_state.selected_field <= 2;
        let is_filtering = state.page == Page::Artists && state.artists.filter_active;

        if is_server_text_field || is_filtering {
            let page = state.page;
            drop(state);
            return match page {
                Page::Server => self.handle_server_key(key).await,
                Page::Artists => self.handle_artists_key(key).await,
                _ => Ok(()),
            };
        }

        // Global keybindings
        match (key.code, key.modifiers) {
            // Quit
            (KeyCode::Char('q'), KeyModifiers::NONE) => {
                state.should_quit = true;
                return Ok(());
            }
            // Page switching
            (KeyCode::F(1), _) => {
                state.page = Page::Songs;
                return Ok(());
            }
            (KeyCode::F(2), _) => {
                state.page = Page::Artists;
                return Ok(());
            }
            (KeyCode::F(3), _) => {
                state.page = Page::Queue;
                return Ok(());
            }
            (KeyCode::F(4), _) => {
                state.page = Page::Playlists;
                return Ok(());
            }
            (KeyCode::F(5), _) => {
                state.page = Page::Server;
                return Ok(());
            }
            (KeyCode::F(6), _) => {
                state.page = Page::Settings;
                return Ok(());
            }
            // Playback controls (global)
            (KeyCode::Char('p'), KeyModifiers::NONE) | (KeyCode::Char(' '), KeyModifiers::NONE) => {
                // Toggle pause
                drop(state);
                return self.toggle_pause().await;
            }
            (KeyCode::Char('l'), KeyModifiers::NONE) => {
                // Next track
                drop(state);
                return self.next_track().await;
            }
            (KeyCode::Char('h'), KeyModifiers::NONE) => {
                // Previous track
                drop(state);
                return self.prev_track().await;
            }
            // Cycle theme (global)
            (KeyCode::Char('t'), KeyModifiers::NONE) => {
                state.settings_state.next_theme();
                state.config.theme = state.settings_state.theme_name().to_string();
                let label = state.settings_state.theme_name().to_string();
                state.notify(format!("Theme: {}", label));
                let _ = state.config.save_default();
                let cava_enabled = state.settings_state.cava_enabled;
                let td = state.settings_state.current_theme();
                let g = td.cava_gradient.clone();
                let h = td.cava_horizontal_gradient.clone();
                let cs = state.settings_state.cava_size as u32;
                drop(state);
                if cava_enabled {
                    self.start_cava(&g, &h, cs);
                }
                return Ok(());
            }
            // Ctrl+R to refresh data from server
            (KeyCode::Char('r'), KeyModifiers::CONTROL) => {
                state.notify("Refreshing...");
                drop(state);
                self.load_initial_data().await;
                let mut state = self.state.write().await;
                state.notify("Data refreshed");
                return Ok(());
            }
            _ => {}
        }

        // Page-specific keybindings
        let page = state.page;
        drop(state);
        match page {
            Page::Songs => self.handle_songs_key(key).await,
            Page::Artists => self.handle_artists_key(key).await,
            Page::Queue => self.handle_queue_key(key).await,
            Page::Playlists => self.handle_playlists_key(key).await,
            Page::Server => self.handle_server_key(key).await,
            Page::Settings => self.handle_settings_key(key).await,
        }
    }
}
