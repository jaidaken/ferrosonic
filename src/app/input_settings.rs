use crossterm::event::{self, KeyCode};

use crate::error::Error;

use super::*;

impl App {
    /// Handle settings page keys
    pub(super) async fn handle_settings_key(&mut self, key: event::KeyEvent) -> Result<(), Error> {
        let mut config_changed = false;

        {
            let mut state = self.state.write().await;
            let field = state.client.settings_state.selected_field;

            match key.code {
                // Navigate between fields
                KeyCode::Up | KeyCode::Char('k') => {
                    if field > 0 {
                        state.client.settings_state.selected_field = field - 1;
                    }
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    if field < 2 {
                        state.client.settings_state.selected_field = field + 1;
                    }
                }
                // Left
                KeyCode::Left | KeyCode::Char('h') => match field {
                    0 => {
                        state.client.settings_state.prev_theme();
                        state.daemon.config.theme = state.client.settings_state.theme_name().to_string();
                        let label = state.client.settings_state.theme_name().to_string();
                        state.client.notify(format!("Theme: {}", label));
                        config_changed = true;
                    }
                    1 if state.client.cava_available => {
                        state.client.settings_state.cava_enabled = !state.client.settings_state.cava_enabled;
                        state.daemon.config.cava = state.client.settings_state.cava_enabled;
                        let status = if state.client.settings_state.cava_enabled {
                            "On"
                        } else {
                            "Off"
                        };
                        state.client.notify(format!("Cava: {}", status));
                        config_changed = true;
                    }
                    2 if state.client.cava_available => {
                        let cur = state.client.settings_state.cava_size;
                        if cur > 10 {
                            let new_size = cur - 5;
                            state.client.settings_state.cava_size = new_size;
                            state.daemon.config.cava_size = new_size;
                            state.client.notify(format!("Cava Size: {}%", new_size));
                            config_changed = true;
                        }
                    }
                    _ => {}
                },
                // Right / Enter / Space
                KeyCode::Right | KeyCode::Char('l') | KeyCode::Enter | KeyCode::Char(' ') => {
                    match field {
                        0 => {
                            state.client.settings_state.next_theme();
                            state.daemon.config.theme = state.client.settings_state.theme_name().to_string();
                            let label = state.client.settings_state.theme_name().to_string();
                            state.client.notify(format!("Theme: {}", label));
                            config_changed = true;
                        }
                        1 if state.client.cava_available => {
                            state.client.settings_state.cava_enabled = !state.client.settings_state.cava_enabled;
                            state.daemon.config.cava = state.client.settings_state.cava_enabled;
                            let status = if state.client.settings_state.cava_enabled {
                                "On"
                            } else {
                                "Off"
                            };
                            state.client.notify(format!("Cava: {}", status));
                            config_changed = true;
                        }
                        2 if state.client.cava_available => {
                            let cur = state.client.settings_state.cava_size;
                            if cur < 80 {
                                let new_size = cur + 5;
                                state.client.settings_state.cava_size = new_size;
                                state.daemon.config.cava_size = new_size;
                                state.client.notify(format!("Cava Size: {}%", new_size));
                                config_changed = true;
                            }
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }

        if config_changed {
            // Save config
            let state = self.state.read().await;
            if let Err(e) = state.daemon.config.save_default() {
                drop(state);
                let mut state = self.state.write().await;
                state.client.notify_error(format!("Failed to save: {}", e));
            } else {
                // Start/stop cava based on new setting, or restart on theme change
                let cava_enabled = state.client.settings_state.cava_enabled;
                let td = state.client.settings_state.current_theme();
                let g = td.cava_gradient.clone();
                let h = td.cava_horizontal_gradient.clone();
                let cs = state.client.settings_state.cava_size as u32;
                let cava_running = self.cava_parser.is_some();
                drop(state);
                if cava_enabled {
                    // (Re)start cava — picks up new theme colors or toggle-on
                    self.start_cava(&g, &h, cs);
                } else if cava_running {
                    self.stop_cava();
                    let mut state = self.state.write().await;
                    state.client.cava_screen.clear();
                }
            }
        }

        Ok(())
    }
}
