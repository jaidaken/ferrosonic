use crossterm::event::{self, KeyCode};
use tracing::info;

use crate::error::Error;
use crate::subsonic::SubsonicClient;

use super::*;

impl App {
    /// Handle server page keys
    pub(super) async fn handle_server_key(&mut self, key: event::KeyEvent) -> Result<(), Error> {
        let mut state = self.state.write().await;

        let field = state.client.server_state.selected_field;
        let is_text_field = field <= 2;

        match key.code {
            // Navigation - always works
            KeyCode::Up => {
                if field > 0 {
                    state.client.server_state.selected_field -= 1;
                }
            }
            KeyCode::Down => {
                if field < 4 {
                    state.client.server_state.selected_field += 1;
                }
            }
            KeyCode::Tab => {
                // Tab moves to next field, wrapping around
                state.client.server_state.selected_field = (field + 1) % 5;
            }
            // Text input for text fields (0=URL, 1=Username, 2=Password)
            KeyCode::Char(c) if is_text_field => match field {
                0 => state.client.server_state.base_url.push(c),
                1 => state.client.server_state.username.push(c),
                2 => state.client.server_state.password.push(c),
                _ => {}
            },
            KeyCode::Backspace if is_text_field => match field {
                0 => {
                    state.client.server_state.base_url.pop();
                }
                1 => {
                    state.client.server_state.username.pop();
                }
                2 => {
                    state.client.server_state.password.pop();
                }
                _ => {}
            },
            // Enter activates buttons, ignored on text fields
            KeyCode::Enter => {
                match field {
                    3 => {
                        // Test connection
                        let url = state.client.server_state.base_url.clone();
                        let user = state.client.server_state.username.clone();
                        let pass = state.client.server_state.password.clone();
                        state.client.server_state.status = Some("Testing connection...".to_string());
                        drop(state);

                        match SubsonicClient::new(&url, &user, &pass) {
                            Ok(client) => match client.ping().await {
                                Ok(_) => {
                                    let mut state = self.state.write().await;
                                    state.client.server_state.status =
                                        Some("Connection successful!".to_string());
                                }
                                Err(e) => {
                                    let mut state = self.state.write().await;
                                    state.client.server_state.status =
                                        Some(format!("Connection failed: {}", e));
                                }
                            },
                            Err(e) => {
                                let mut state = self.state.write().await;
                                state.client.server_state.status = Some(format!("Invalid URL: {}", e));
                            }
                        }
                        return Ok(());
                    }
                    4 => {
                        // Save config and reconnect
                        info!(
                            "Saving config: url='{}', user='{}'",
                            state.client.server_state.base_url, state.client.server_state.username
                        );
                        state.daemon.config.base_url = state.client.server_state.base_url.clone();
                        state.daemon.config.username = state.client.server_state.username.clone();
                        state.daemon.config.password = state.client.server_state.password.clone();

                        let url = state.daemon.config.base_url.clone();
                        let user = state.daemon.config.username.clone();
                        let pass = state.daemon.config.password.clone();

                        match state.daemon.config.save_default() {
                            Ok(_) => {
                                info!("Config saved successfully");
                                state.client.server_state.status =
                                    Some("Saved! Connecting...".to_string());
                            }
                            Err(e) => {
                                info!("Config save failed: {}", e);
                                state.client.server_state.status = Some(format!("Save failed: {}", e));
                                return Ok(());
                            }
                        }
                        drop(state);

                        // Create new client and load data
                        match SubsonicClient::new(&url, &user, &pass) {
                            Ok(client) => {
                                self.subsonic = Some(client);
                                self.load_initial_data().await;
                                let mut state = self.state.write().await;
                                state.client.server_state.status =
                                    Some("Connected and loaded data!".to_string());
                            }
                            Err(e) => {
                                let mut state = self.state.write().await;
                                state.client.server_state.status =
                                    Some(format!("Saved but connection failed: {}", e));
                            }
                        }
                        return Ok(());
                    }
                    _ => {} // Ignore Enter on text fields (handles paste with newlines)
                }
            }
            _ => {}
        }

        Ok(())
    }
}
