use crossterm::event::{self, KeyCode};

use crate::error::Error;

use super::*;

/// Which setting changed in this keystroke. Drives both the daemon
/// request that gets sent and the client-side cava restart decision.
#[derive(Clone, Copy)]
enum SettingChange {
    Theme,
    Cava,
    CavaSize,
    Daemon,
    AutoContinue,
}

/// Number of settings fields. Used as the upper bound for the down-key
/// navigation (`selected_field < SETTINGS_FIELD_COUNT - 1`).
const SETTINGS_FIELD_COUNT: usize = 5;

impl App {
    /// Handle settings page keys
    pub(super) async fn handle_settings_key(&mut self, key: event::KeyEvent) -> Result<(), Error> {
        let mut change: Option<SettingChange> = None;

        {
            let ds = self.daemon_state.read().await;
            let mut cs = self.client_state.write().await;
            let state = AppState { daemon: &*ds, client: &mut *cs };
            let field = state.client.settings_state.selected_field;

            match key.code {
                // Navigate between fields
                KeyCode::Up | KeyCode::Char('k') => {
                    if field > 0 {
                        state.client.settings_state.selected_field = field - 1;
                    }
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    if field < SETTINGS_FIELD_COUNT - 1 {
                        state.client.settings_state.selected_field = field + 1;
                    }
                }
                // Left
                KeyCode::Left | KeyCode::Char('h') => match field {
                    0 => {
                        state.client.settings_state.prev_theme();
                        let label = state.client.settings_state.theme_name().to_string();
                        state.client.notify(format!("Theme: {}", label));
                        change = Some(SettingChange::Theme);
                    }
                    1 if state.client.cava_available => {
                        state.client.settings_state.cava_enabled = !state.client.settings_state.cava_enabled;
                        let status = if state.client.settings_state.cava_enabled {
                            "On"
                        } else {
                            "Off"
                        };
                        state.client.notify(format!("Cava: {}", status));
                        change = Some(SettingChange::Cava);
                    }
                    2 if state.client.cava_available => {
                        let cur = state.client.settings_state.cava_size;
                        if cur > 10 {
                            let new_size = cur - 5;
                            state.client.settings_state.cava_size = new_size;
                            state.client.notify(format!("Cava Size: {}%", new_size));
                            change = Some(SettingChange::CavaSize);
                        }
                    }
                    3 => {
                        state.client.settings_state.daemon_enabled =
                            !state.client.settings_state.daemon_enabled;
                        let status = if state.client.settings_state.daemon_enabled {
                            "On"
                        } else {
                            "Off"
                        };
                        state.client.notify(format!(
                            "Daemon: {} (restart to apply)",
                            status
                        ));
                        change = Some(SettingChange::Daemon);
                    }
                    4 => {
                        state.client.settings_state.auto_continue =
                            !state.client.settings_state.auto_continue;
                        let status = if state.client.settings_state.auto_continue {
                            "On"
                        } else {
                            "Off"
                        };
                        state.client.notify(format!("Auto-continue: {}", status));
                        change = Some(SettingChange::AutoContinue);
                    }
                    _ => {}
                },
                // Right / Enter / Space
                KeyCode::Right | KeyCode::Char('l') | KeyCode::Enter | KeyCode::Char(' ') => {
                    match field {
                        0 => {
                            state.client.settings_state.next_theme();
                            let label = state.client.settings_state.theme_name().to_string();
                            state.client.notify(format!("Theme: {}", label));
                            change = Some(SettingChange::Theme);
                        }
                        1 if state.client.cava_available => {
                            state.client.settings_state.cava_enabled = !state.client.settings_state.cava_enabled;
                            let status = if state.client.settings_state.cava_enabled {
                                "On"
                            } else {
                                "Off"
                            };
                            state.client.notify(format!("Cava: {}", status));
                            change = Some(SettingChange::Cava);
                        }
                        2 if state.client.cava_available => {
                            let cur = state.client.settings_state.cava_size;
                            if cur < 80 {
                                let new_size = cur + 5;
                                state.client.settings_state.cava_size = new_size;
                                state.client.notify(format!("Cava Size: {}%", new_size));
                                change = Some(SettingChange::CavaSize);
                            }
                        }
                        3 => {
                            state.client.settings_state.daemon_enabled =
                                !state.client.settings_state.daemon_enabled;
                            let status = if state.client.settings_state.daemon_enabled {
                                "On"
                            } else {
                                "Off"
                            };
                            state.client.notify(format!(
                                "Daemon: {} (restart to apply)",
                                status
                            ));
                            change = Some(SettingChange::Daemon);
                        }
                        4 => {
                            state.client.settings_state.auto_continue =
                                !state.client.settings_state.auto_continue;
                            let status = if state.client.settings_state.auto_continue {
                                "On"
                            } else {
                                "Off"
                            };
                            state.client.notify(format!("Auto-continue: {}", status));
                            change = Some(SettingChange::AutoContinue);
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }

        let Some(change) = change else {
            return Ok(());
        };

        // Snapshot the new client-side values, then dispatch the
        // matching daemon request (it persists + emits ConfigChanged).
        let (theme_name, cava_enabled, cava_size, daemon_enabled, auto_continue, gradient, h_gradient) = {
            let ds = self.daemon_state.read().await;
            let mut cs = self.client_state.write().await;
            let state = AppState { daemon: &*ds, client: &mut *cs };
            let s = &state.client.settings_state;
            (
                s.theme_name().to_string(),
                s.cava_enabled,
                s.cava_size,
                s.daemon_enabled,
                s.auto_continue,
                s.current_theme().cava_gradient.clone(),
                s.current_theme().cava_horizontal_gradient.clone(),
            )
        };
        let req = match change {
            SettingChange::Theme => DaemonRequest::SetTheme(theme_name),
            SettingChange::Cava => DaemonRequest::SetCavaEnabled(cava_enabled),
            SettingChange::CavaSize => DaemonRequest::SetCavaSize(cava_size),
            SettingChange::Daemon => DaemonRequest::SetDaemonEnabled(daemon_enabled),
            SettingChange::AutoContinue => DaemonRequest::SetAutoContinue(auto_continue),
        };
        if let Err(e) = self.client.request(req).await {
            let ds = self.daemon_state.read().await;
            let mut cs = self.client_state.write().await;
            let state = AppState { daemon: &*ds, client: &mut *cs };
            state.client.notify_error(format!("Failed to save: {}", e));
            return Ok(());
        }

        // Cava lifecycle stays client-side — start/stop/restart based
        // on what changed. Daemon toggle does not affect cava.
        let cava_running = self.cava_parser.is_some();
        let cava_h = cava_size as u32;
        match change {
            SettingChange::Cava => {
                if cava_enabled {
                    self.start_cava(&gradient, &h_gradient, cava_h);
                } else if cava_running {
                    self.stop_cava();
                    let ds = self.daemon_state.read().await;
                    let mut cs = self.client_state.write().await;
                    let state = AppState { daemon: &*ds, client: &mut *cs };
                    state.client.cava_screen.clear();
                }
            }
            SettingChange::Theme | SettingChange::CavaSize => {
                if cava_enabled {
                    // Restart with new theme colors / size
                    self.start_cava(&gradient, &h_gradient, cava_h);
                }
            }
            SettingChange::Daemon | SettingChange::AutoContinue => {}
        }

        Ok(())
    }
}
