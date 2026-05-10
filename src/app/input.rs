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
                if self.cava_parser.is_some() {
                    let (g, h, cava_h) = {
                        let cs = self.client_state.read().await;
                        let td = cs.settings_state.current_theme();
                        (
                            td.cava_gradient.clone(),
                            td.cava_horizontal_gradient.clone(),
                            cs.settings_state.cava_size as u32,
                        )
                    };
                    self.start_cava(&g, &h, cava_h);
                    let mut cs = self.client_state.write().await;
                    cs.cava_screen.clear();
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }

    /// Handle keyboard input
    pub(super) async fn handle_key(&mut self, key: event::KeyEvent) -> Result<(), Error> {
        let ds = self.daemon_state.read().await;
        let mut cs = self.client_state.write().await;
        let state = AppState { daemon: &*ds, client: &mut *cs };

        // Clear notification on any keypress
        state.client.clear_notification();

        // F-keys always switch pages, even while typing in a text input.
        // Discard any unsaved edits on the way out so the form reverts
        // to the saved config / filter state on return.
        let is_function_key = matches!(key.code, KeyCode::F(_));
        if is_function_key {
            if state.client.page == Page::Server {
                let cfg = state.daemon.config.clone();
                state.client.server_state.base_url = cfg.base_url;
                state.client.server_state.username = cfg.username;
                state.client.server_state.password = cfg.password;
                state.client.server_state.status = None;
            }
            if state.client.page == Page::Library && state.client.artists.filter_active {
                state.client.artists.filter_active = false;
            }
        } else {
            let is_server_text_field =
                state.client.page == Page::Server && state.client.server_state.selected_field <= 2;
            let is_filtering =
                state.client.page == Page::Library && state.client.artists.filter_active;

            if is_server_text_field || is_filtering {
                let page = state.client.page;
                drop(state); drop(cs); drop(ds);
                return match page {
                    Page::Server => self.handle_server_key(key).await,
                    Page::Library => self.handle_artists_key(key).await,
                    _ => Ok(()),
                };
            }
        }

        // Global keybindings
        match (key.code, key.modifiers) {
            // Quit
            (KeyCode::Char('q'), KeyModifiers::NONE) => {
                state.client.should_quit = true;
                return Ok(());
            }
            // Page switching
            (KeyCode::F(1), _) => {
                state.client.page = Page::Library;
                return Ok(());
            }
            (KeyCode::F(2), _) => {
                state.client.page = Page::Queue;
                return Ok(());
            }
            (KeyCode::F(3), _) => {
                state.client.page = Page::QuickPlay;
                return Ok(());
            }
            (KeyCode::F(4), _) => {
                state.client.page = Page::Playlists;
                return Ok(());
            }
            (KeyCode::F(5), _) => {
                state.client.page = Page::Server;
                return Ok(());
            }
            (KeyCode::F(6), _) => {
                state.client.page = Page::Settings;
                return Ok(());
            }
            // Playback controls (global)
            (KeyCode::Char('p'), KeyModifiers::NONE) | (KeyCode::Char(' '), KeyModifiers::NONE) => {
                // Toggle pause
                drop(state); drop(cs); drop(ds);
                return self.client.request(DaemonRequest::TogglePause).await.map(|_| ()).map_err(Error::from);
            }
            (KeyCode::Char('l'), KeyModifiers::NONE) => {
                // Next track
                drop(state); drop(cs); drop(ds);
                return self.client.request(DaemonRequest::Next).await.map(|_| ()).map_err(Error::from);
            }
            (KeyCode::Char('h'), KeyModifiers::NONE) => {
                // Previous track
                drop(state); drop(cs); drop(ds);
                return self.client.request(DaemonRequest::Previous).await.map(|_| ()).map_err(Error::from);
            }
            // Cycle theme (global)
            (KeyCode::Char('t'), KeyModifiers::NONE) => {
                state.client.settings_state.next_theme();
                let theme_name = state.client.settings_state.theme_name().to_string();
                state.client.notify(format!("Theme: {}", theme_name));
                let cava_enabled = state.client.settings_state.cava_enabled;
                let td = state.client.settings_state.current_theme();
                let g = td.cava_gradient.clone();
                let h = td.cava_horizontal_gradient.clone();
                let cava_h = state.client.settings_state.cava_size as u32;
                drop(state); drop(cs); drop(ds);
                let _ = self
                    .client
                    .request(DaemonRequest::SetTheme(theme_name))
                    .await;
                if cava_enabled {
                    self.start_cava(&g, &h, cava_h);
                }
                return Ok(());
            }
            // Toggle star on currently-playing song
            (KeyCode::Char('n'), KeyModifiers::NONE) => {
                let song_id = state.daemon.now_playing.song.as_ref().map(|s| s.id.clone());
                drop(state); drop(cs); drop(ds);
                if let Some(id) = song_id {
                    let _ = self
                        .client
                        .request(DaemonRequest::ToggleStarSong(id))
                        .await;
                }
                return Ok(());
            }
            // Ctrl+R to refresh data from server
            (KeyCode::Char('r'), KeyModifiers::CONTROL) => {
                state.client.notify("Refreshing...");
                drop(state); drop(cs); drop(ds);
                self.load_initial_data().await;
                let ds = self.daemon_state.read().await;
                let mut cs = self.client_state.write().await;
                let state = AppState { daemon: &*ds, client: &mut *cs };
                state.client.notify("Data refreshed");
                return Ok(());
            }
            _ => {}
        }

        // Page-specific keybindings
        let page = state.client.page;
        drop(state); drop(cs); drop(ds);
        match page {
            Page::QuickPlay => self.handle_songs_key(key).await,
            Page::Library => self.handle_artists_key(key).await,
            Page::Queue => self.handle_queue_key(key).await,
            Page::Playlists => self.handle_playlists_key(key).await,
            Page::Server => self.handle_server_key(key).await,
            Page::Settings => self.handle_settings_key(key).await,
        }
    }
}
