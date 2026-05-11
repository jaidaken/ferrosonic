use crossterm::event::{self, KeyCode};

use crate::error::Error;

use super::*;

#[derive(Clone, Copy)]
enum SettingChange {
    Theme,
    Cava,
    CavaSize,
    CoverArt,
    CoverArtSize,
    Repeat,
    AutoContinue,
    Daemon,
}

const SETTINGS_FIELD_COUNT: usize = 8;

impl App {
    pub(super) async fn handle_settings_key(&mut self, key: event::KeyEvent) -> Result<(), Error> {
        let mut change: Option<SettingChange> = None;

        {
            let _ds = self.daemon_state.read().await;
            let mut cs = self.client_state.write().await;
            let field = cs.settings_state.selected_field;
            let cava_ok = cs.cava_available;

            match key.code {
                KeyCode::Up | KeyCode::Char('k') => {
                    if field > 0 {
                        cs.settings_state.selected_field = field - 1;
                    }
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    if field < SETTINGS_FIELD_COUNT - 1 {
                        cs.settings_state.selected_field = field + 1;
                    }
                }
                KeyCode::Left | KeyCode::Char('h') => {
                    change = adjust_setting(&mut cs.settings_state, field, -1, cava_ok);
                    if let Some(c) = change {
                        let msg = change_message(&cs.settings_state, c);
                        cs.notify(msg);
                    }
                }
                KeyCode::Right | KeyCode::Char('l') | KeyCode::Enter | KeyCode::Char(' ') => {
                    change = adjust_setting(&mut cs.settings_state, field, 1, cava_ok);
                    if let Some(c) = change {
                        let msg = change_message(&cs.settings_state, c);
                        cs.notify(msg);
                    }
                }
                _ => {}
            }
        }

        let Some(change) = change else {
            return Ok(());
        };

        let (
            theme_name,
            cava_enabled,
            cava_size,
            cover_art,
            cover_art_size,
            repeat_mode,
            auto_continue,
            daemon_enabled,
            gradient,
            h_gradient,
        ) = {
            let ds = self.daemon_state.read().await;
            let mut cs = self.client_state.write().await;
            let state = AppState {
                daemon: &ds,
                client: &mut cs,
            };
            let s = &state.client.settings_state;
            (
                s.theme_name().to_string(),
                s.cava_enabled,
                s.cava_size,
                s.cover_art,
                s.cover_art_size,
                s.repeat_mode,
                s.auto_continue,
                s.daemon_enabled,
                s.current_theme().cava_gradient.clone(),
                s.current_theme().cava_horizontal_gradient.clone(),
            )
        };
        let req = match change {
            SettingChange::Theme => DaemonRequest::SetTheme(theme_name),
            SettingChange::Cava => DaemonRequest::SetCavaEnabled(cava_enabled),
            SettingChange::CavaSize => DaemonRequest::SetCavaSize(cava_size),
            SettingChange::CoverArt => DaemonRequest::SetCoverArtEnabled(cover_art),
            SettingChange::CoverArtSize => DaemonRequest::SetCoverArtSize(cover_art_size),
            SettingChange::Repeat => DaemonRequest::SetRepeatMode(repeat_mode),
            SettingChange::AutoContinue => DaemonRequest::SetAutoContinue(auto_continue),
            SettingChange::Daemon => DaemonRequest::SetDaemonEnabled(daemon_enabled),
        };
        if let Err(e) = self.client.request(req).await {
            let ds = self.daemon_state.read().await;
            let mut cs = self.client_state.write().await;
            let state = AppState {
                daemon: &ds,
                client: &mut cs,
            };
            state.client.notify_error(format!("Failed to save: {}", e));
            return Ok(());
        }

        // Cava lifecycle is client-side; daemon toggle doesn't affect it.
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
                    let state = AppState {
                        daemon: &ds,
                        client: &mut cs,
                    };
                    state.client.cava_screen.clear();
                }
            }
            SettingChange::Theme | SettingChange::CavaSize => {
                if cava_enabled {
                    self.start_cava(&gradient, &h_gradient, cava_h);
                }
            }
            SettingChange::CoverArt
            | SettingChange::CoverArtSize
            | SettingChange::Repeat
            | SettingChange::AutoContinue
            | SettingChange::Daemon => {}
        }

        Ok(())
    }
}

/// `step`: -1 for left, +1 for right/enter. Mutates the settings
/// state and returns the matching `SettingChange` so the caller can
/// dispatch + notify.
fn adjust_setting(
    s: &mut crate::app::state::SettingsState,
    field: usize,
    step: i32,
    cava_ok: bool,
) -> Option<SettingChange> {
    use crate::config::RepeatMode;
    match field {
        0 => {
            if step < 0 {
                s.prev_theme();
            } else {
                s.next_theme();
            }
            Some(SettingChange::Theme)
        }
        1 if cava_ok => {
            s.cava_enabled = !s.cava_enabled;
            Some(SettingChange::Cava)
        }
        2 if cava_ok => {
            let cur = s.cava_size as i32;
            let new = (cur + step * 5).clamp(10, 80) as u8;
            if new != s.cava_size {
                s.cava_size = new;
                Some(SettingChange::CavaSize)
            } else {
                None
            }
        }
        3 => {
            s.cover_art = !s.cover_art;
            Some(SettingChange::CoverArt)
        }
        4 => {
            let cur = s.cover_art_size as i32;
            let new = (cur + step * 2).clamp(8, 24) as u8;
            if new != s.cover_art_size {
                s.cover_art_size = new;
                Some(SettingChange::CoverArtSize)
            } else {
                None
            }
        }
        5 => {
            // Left and right both cycle; left goes one back, right one forward.
            s.repeat_mode = if step < 0 {
                match s.repeat_mode {
                    RepeatMode::Off => RepeatMode::All,
                    RepeatMode::One => RepeatMode::Off,
                    RepeatMode::All => RepeatMode::One,
                }
            } else {
                s.repeat_mode.cycle()
            };
            Some(SettingChange::Repeat)
        }
        6 => {
            s.auto_continue = !s.auto_continue;
            Some(SettingChange::AutoContinue)
        }
        7 => {
            s.daemon_enabled = !s.daemon_enabled;
            Some(SettingChange::Daemon)
        }
        _ => None,
    }
}

fn change_message(s: &crate::app::state::SettingsState, change: SettingChange) -> String {
    match change {
        SettingChange::Theme => format!("Theme: {}", s.theme_name()),
        SettingChange::Cava => format!("Cava: {}", on_off(s.cava_enabled)),
        SettingChange::CavaSize => format!("Cava Size: {}%", s.cava_size),
        SettingChange::CoverArt => format!("Cover Art: {}", on_off(s.cover_art)),
        SettingChange::CoverArtSize => format!("Cover Art Size: {} rows", s.cover_art_size),
        SettingChange::Repeat => format!("Repeat: {}", s.repeat_mode.label()),
        SettingChange::AutoContinue => format!("Auto-continue: {}", on_off(s.auto_continue)),
        SettingChange::Daemon => format!("Daemon: {} (restart to apply)", on_off(s.daemon_enabled)),
    }
}

fn on_off(v: bool) -> &'static str {
    if v {
        "On"
    } else {
        "Off"
    }
}
