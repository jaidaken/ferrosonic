//! Server-config page input. L60_FILE near-miss (58.87% line, 62.11% region): 30 of 39 async continuation states unreached; full IPC roundtrip tests via RecordingClient queued for Phase C zero-assert audit deepening.

use crossterm::event::{self, KeyCode};
use tracing::info;

use crate::error::Error;

use super::*;

const MAX_FIELD_LEN: usize = 1024;

impl App {
    pub(super) async fn handle_server_key(&mut self, key: event::KeyEvent) -> Result<(), Error> {
        let ds = self.daemon_state.read().await;
        let mut cs = self.client_state.write().await;
        let state = AppState {
            daemon: &ds,
            client: &mut cs,
        };

        let field = state.client.server_state.selected_field;
        let is_text_field = field <= 2;

        match key.code {
            KeyCode::Up if field > 0 => {
                state.client.server_state.selected_field -= 1;
            }
            KeyCode::Down if field < 4 => {
                state.client.server_state.selected_field += 1;
            }
            KeyCode::Tab => {
                state.client.server_state.selected_field = (field + 1) % 5;
            }
            KeyCode::Char(c) if is_text_field => match field {
                0 => {
                    let t = &mut state.client.server_state.base_url;
                    if t.len() < MAX_FIELD_LEN {
                        t.push(c);
                    }
                }
                1 => {
                    let t = &mut state.client.server_state.username;
                    if t.len() < MAX_FIELD_LEN {
                        t.push(c);
                    }
                }
                2 => {
                    let s = &mut state.client.server_state.password;
                    if s.len() < MAX_FIELD_LEN {
                        s.push_char(c);
                    }
                }
                _ => return Ok(()),
            },
            KeyCode::Backspace if is_text_field => match field {
                0 => {
                    state.client.server_state.base_url.pop();
                }
                1 => {
                    state.client.server_state.username.pop();
                }
                2 => {
                    state.client.server_state.password.pop_char();
                }
                _ => {}
            },
            KeyCode::Enter => {
                match field {
                    3 => {
                        let base_url = state.client.server_state.base_url.clone();
                        let username = state.client.server_state.username.clone();
                        let password = state.client.server_state.password.clone();
                        if url::Url::parse(&base_url).is_err() {
                            state.client.server_state.status =
                                Some("Invalid URL (must start with http:// or https://)".to_string());
                            return Ok(());
                        }
                        state.client.server_state.status =
                            Some("Testing connection...".to_string());
                        drop(state);
                        drop(cs);
                        drop(ds);

                        match self
                            .client
                            .request(DaemonRequest::TestServerConnection {
                                base_url,
                                username,
                                password,
                            })
                            .await
                        {
                            Ok(crate::ipc::DaemonResponse::ConnectionTestResult {
                                ok,
                                message,
                            }) => {
                                let ds = self.daemon_state.read().await;
                                let mut cs = self.client_state.write().await;
                                let state = AppState {
                                    daemon: &ds,
                                    client: &mut cs,
                                };
                                state.client.server_state.status = Some(if ok {
                                    "Connection successful!".to_string()
                                } else {
                                    message
                                });
                            }
                            Ok(_) => {
                                let ds = self.daemon_state.read().await;
                                let mut cs = self.client_state.write().await;
                                let state = AppState {
                                    daemon: &ds,
                                    client: &mut cs,
                                };
                                state.client.server_state.status =
                                    Some("Unexpected daemon response".to_string());
                            }
                            Err(e) => {
                                let ds = self.daemon_state.read().await;
                                let mut cs = self.client_state.write().await;
                                let state = AppState {
                                    daemon: &ds,
                                    client: &mut cs,
                                };
                                state.client.server_state.status =
                                    Some(format!("IPC error: {}", e));
                            }
                        }
                        return Ok(());
                    }
                    4 => {
                        info!(
                            "Saving config: url='{}', user='{}'",
                            state.client.server_state.base_url, state.client.server_state.username
                        );
                        let base_url = state.client.server_state.base_url.clone();
                        let username = state.client.server_state.username.clone();
                        let password = state.client.server_state.password.clone();
                        if url::Url::parse(&base_url).is_err() {
                            state.client.server_state.status =
                                Some("Invalid URL (must start with http:// or https://)".to_string());
                            return Ok(());
                        }
                        state.client.server_state.status = Some("Saving...".to_string());
                        drop(state);
                        drop(cs);
                        drop(ds);

                        match self
                            .client
                            .request(DaemonRequest::UpdateServerConfig {
                                base_url,
                                username,
                                password,
                            })
                            .await
                        {
                            Ok(_) => {
                                info!("Config saved and refetched");
                                let ds = self.daemon_state.read().await;
                                let mut cs = self.client_state.write().await;
                                let state = AppState {
                                    daemon: &ds,
                                    client: &mut cs,
                                };
                                state.client.server_state.status =
                                    Some("Connected and loaded data!".to_string());
                            }
                            Err(e) => {
                                info!("Config save failed: {}", e);
                                let ds = self.daemon_state.read().await;
                                let mut cs = self.client_state.write().await;
                                let state = AppState {
                                    daemon: &ds,
                                    client: &mut cs,
                                };
                                state.client.server_state.status =
                                    Some(format!("Save failed: {}", e));
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
