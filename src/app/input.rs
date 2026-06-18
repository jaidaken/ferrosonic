use crossterm::event::{self, Event, KeyCode, KeyModifiers};

use crate::error::Error;

use super::*;

impl App {
    /// Handle terminal events. Pub for integration tests; production
    /// callers use `App::run`.
    pub async fn handle_event(&mut self, event: Event) -> Result<(), Error> {
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

    /// Route one key event to the active page handler.
    pub async fn handle_key(&mut self, key: event::KeyEvent) -> Result<(), Error> {
        let ds = self.daemon_state.read().await;
        let mut cs = self.client_state.write().await;
        let state = AppState {
            daemon: &ds,
            client: &mut cs,
        };

        state.client.clear_notification();

        // Quit-confirm prompt: while it is up, only y/n/esc do anything.
        if state.client.quit_prompt {
            match key.code {
                KeyCode::Char('y') | KeyCode::Char('Y') => {
                    state.client.quit_prompt = false;
                    state.client.should_quit = true;
                    drop(state);
                    drop(cs);
                    drop(ds);
                    let _ = self.client.request(DaemonRequest::Shutdown).await;
                    return Ok(());
                }
                KeyCode::Char('n') | KeyCode::Char('N') => {
                    state.client.quit_prompt = false;
                    state.client.should_quit = true;
                    return Ok(());
                }
                KeyCode::Esc => {
                    state.client.quit_prompt = false;
                    return Ok(());
                }
                _ => return Ok(()),
            }
        }

        // F-keys switch pages while typing; unsaved edits revert.
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
                drop(state);
                drop(cs);
                drop(ds);
                return match page {
                    Page::Server => self.handle_server_key(key).await,
                    Page::Library => self.handle_library_key(key).await,
                    _ => Ok(()),
                };
            }
        }

        match (key.code, key.modifiers) {
            (KeyCode::Char('q'), KeyModifiers::NONE) => {
                // Results showing but box not capturing: q backs out to the
                // tree. An active box routes q to the filter above (types q).
                if state.client.page == Page::Library && !state.client.artists.filter.is_empty() {
                    state.client.artists.exit_search();
                    return Ok(());
                }
                // A separate daemon outlives the TUI, so ask whether to stop it.
                if state.client.daemon_backed {
                    state.client.quit_prompt = true;
                } else {
                    state.client.should_quit = true;
                }
                return Ok(());
            }
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
            (KeyCode::Char('p'), KeyModifiers::NONE) | (KeyCode::Char(' '), KeyModifiers::NONE) => {
                drop(state);
                drop(cs);
                drop(ds);
                return self
                    .client
                    .request(DaemonRequest::TogglePause)
                    .await
                    .map(|_| ())
                    .map_err(Error::from);
            }
            (KeyCode::Char('l'), KeyModifiers::NONE) => {
                drop(state);
                drop(cs);
                drop(ds);
                return self
                    .client
                    .request(DaemonRequest::Next)
                    .await
                    .map(|_| ())
                    .map_err(Error::from);
            }
            (KeyCode::Char('h'), KeyModifiers::NONE) => {
                drop(state);
                drop(cs);
                drop(ds);
                return self
                    .client
                    .request(DaemonRequest::Previous)
                    .await
                    .map(|_| ())
                    .map_err(Error::from);
            }
            (KeyCode::Char('n'), KeyModifiers::NONE) => {
                let song_id = state.daemon.now_playing.song.as_ref().map(|s| s.id.clone());
                drop(state);
                drop(cs);
                drop(ds);
                if let Some(id) = song_id {
                    let _ = self.client.request(DaemonRequest::ToggleStarSong(id)).await;
                }
                return Ok(());
            }
            (KeyCode::Char('T'), _) => {
                state.client.notify("Shuffling library...");
                drop(state);
                drop(cs);
                drop(ds);
                let _ = self.client.request(DaemonRequest::ShuffleLibrary).await;
                return Ok(());
            }
            (KeyCode::Char('r'), m) if !m.contains(KeyModifiers::CONTROL) => {
                let new_mode = state.client.settings_state.repeat_mode.cycle();
                state.client.settings_state.repeat_mode = new_mode;
                state.client.notify(format!("Repeat: {}", new_mode.label()));
                drop(state);
                drop(cs);
                drop(ds);
                let _ = self
                    .client
                    .request(DaemonRequest::SetRepeatMode(new_mode))
                    .await;
                return Ok(());
            }
            (KeyCode::Char('r'), KeyModifiers::CONTROL) => {
                state.client.notify("Refreshing...");
                drop(state);
                drop(cs);
                drop(ds);
                self.load_initial_data().await;
                let ds = self.daemon_state.read().await;
                let mut cs = self.client_state.write().await;
                let state = AppState {
                    daemon: &ds,
                    client: &mut cs,
                };
                state.client.notify("Data refreshed");
                return Ok(());
            }
            _ => {}
        }

        let page = state.client.page;
        drop(state);
        drop(cs);
        drop(ds);
        match page {
            Page::QuickPlay => self.handle_songs_key(key).await,
            Page::Library => self.handle_library_key(key).await,
            Page::Queue => self.handle_queue_key(key).await,
            Page::Playlists => self.handle_playlists_key(key).await,
            Page::Server => self.handle_server_key(key).await,
            Page::Settings => self.handle_settings_key(key).await,
        }
    }
}
