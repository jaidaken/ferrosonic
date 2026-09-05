use crossterm::event::{self, KeyCode};

use crate::error::Error;

use super::{App, AppState, DaemonRequest};

#[derive(Clone, Copy)]
enum SettingChange {
    Theme,
    Cava,
    CavaSize,
    CoverArt,
    CoverArtSize,
    Repeat,
    AutoContinue,
    Scrobble,
    Daemon,
    Notifications,
    ReplayGainMode,
    ReplayGainPreamp,
    ReplayGainClip,
    /// Any of the 5 playback-filter fields (Min Rating, Year Min/Max,
    /// Duration Min/Max); all persist as one `SetPlaybackFilters` request.
    PlaybackFilters,
}

const SETTINGS_FIELD_COUNT: usize = 18;
/// `adjust_setting`'s Left/Right step for the `ReplayGain` preamp, in dB.
const REPLAY_GAIN_PREAMP_STEP: f64 = 0.5;
/// `adjust_setting`'s Left/Right step for the Year Min/Max filters.
const YEAR_FILTER_STEP: i32 = 5;
/// `adjust_setting`'s Left/Right step for the Duration Min/Max filters, in seconds.
const DURATION_FILTER_STEP: i32 = 15;
/// Bounds for the Year Min/Max filter cycle (inclusive).
const YEAR_FILTER_RANGE: (i32, i32) = (1900, 2100);
/// Bounds for the Duration Min/Max filter cycle, in seconds (inclusive).
const DURATION_FILTER_RANGE: (u32, u32) = (0, 3600);

impl App {
    // Cohesive single match/render; splitting would fragment one logical unit.
    #[allow(clippy::too_many_lines)]
    // significant_drop_tightening: tokio guard held to scope; not tightened (early-drop is borrow-blocked, spans a trailing await, or saves nothing before return).
    #[allow(clippy::significant_drop_tightening)]
    pub(super) async fn handle_settings_key(&mut self, key: event::KeyEvent) -> Result<(), Error> {
        let mut change: Option<SettingChange> = None;

        {
            let _ds = self.daemon_state.read().await;
            let mut cs = self.client_state.write().await;
            let field = cs.settings_state.selected_field;
            let cava_ok = cs.cava_available;

            match key.code {
                KeyCode::Up | KeyCode::Char('k') if field > 0 => {
                    cs.settings_state.selected_field = field - 1;
                }
                KeyCode::Down | KeyCode::Char('j') if field < SETTINGS_FIELD_COUNT - 1 => {
                    cs.settings_state.selected_field = field + 1;
                }
                KeyCode::Left | KeyCode::Char('h') => {
                    change = adjust_setting(&mut cs.settings_state, field, -1, cava_ok);
                    if let Some(c) = change {
                        let msg = change_message(&cs.settings_state, c, field);
                        cs.notify(msg);
                    }
                }
                KeyCode::Right | KeyCode::Char('l' | ' ') | KeyCode::Enter => {
                    change = adjust_setting(&mut cs.settings_state, field, 1, cava_ok);
                    if let Some(c) = change {
                        let msg = change_message(&cs.settings_state, c, field);
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
            scrobble,
            daemon_enabled,
            notifications,
            replay_gain_mode,
            replay_gain_preamp,
            replay_gain_clip,
            playback_filters,
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
                s.scrobble,
                s.daemon_enabled,
                s.notifications,
                s.replay_gain_mode,
                s.replay_gain_preamp,
                s.replay_gain_clip,
                s.playback_filters.clone(),
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
            SettingChange::Scrobble => DaemonRequest::SetScrobble(scrobble),
            SettingChange::Daemon => DaemonRequest::SetDaemonEnabled(daemon_enabled),
            SettingChange::Notifications => DaemonRequest::SetNotifications(notifications),
            SettingChange::ReplayGainMode => DaemonRequest::SetReplayGainMode(replay_gain_mode),
            SettingChange::ReplayGainPreamp => {
                DaemonRequest::SetReplayGainPreamp(replay_gain_preamp)
            }
            SettingChange::ReplayGainClip => DaemonRequest::SetReplayGainClip(replay_gain_clip),
            SettingChange::PlaybackFilters => DaemonRequest::SetPlaybackFilters(playback_filters),
        };
        if let Err(e) = self.client.request(req).await {
            let ds = self.daemon_state.read().await;
            let mut cs = self.client_state.write().await;
            let state = AppState {
                daemon: &ds,
                client: &mut cs,
            };
            state.client.notify_error(format!("Failed to save: {e}"));
            return Ok(());
        }

        // Cava lifecycle is client-side; daemon toggle doesn't affect it.
        let cava_running = self.cava_parser.is_some();
        let cava_h = u32::from(cava_size);
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
            | SettingChange::Scrobble
            | SettingChange::Daemon
            | SettingChange::Notifications
            | SettingChange::ReplayGainMode
            | SettingChange::ReplayGainPreamp
            | SettingChange::ReplayGainClip
            | SettingChange::PlaybackFilters => {}
        }

        Ok(())
    }
}

/// `step`: -1 for left, +1 for right/enter. Mutates the settings
/// state and returns the matching `SettingChange` so the caller can
/// dispatch + notify.
// Flat one-arm-per-field dispatcher; splitting would fragment one logical
// unit (matches the too_many_lines exception already used elsewhere, see
// docs/KNOWN-ISSUES.md build-hygiene section).
#[allow(clippy::too_many_lines)]
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
            let cur = i32::from(s.cava_size);
            let new = crate::num::u8_sat((cur + step * 5).clamp(10, 80));
            if new == s.cava_size {
                None
            } else {
                s.cava_size = new;
                Some(SettingChange::CavaSize)
            }
        }
        3 => {
            s.cover_art = !s.cover_art;
            Some(SettingChange::CoverArt)
        }
        4 => {
            let cur = i32::from(s.cover_art_size);
            let new = crate::num::u8_sat((cur + step * 2).clamp(8, 24));
            if new == s.cover_art_size {
                None
            } else {
                s.cover_art_size = new;
                Some(SettingChange::CoverArtSize)
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
            s.scrobble = !s.scrobble;
            Some(SettingChange::Scrobble)
        }
        8 => {
            s.daemon_enabled = !s.daemon_enabled;
            Some(SettingChange::Daemon)
        }
        9 => {
            s.notifications = !s.notifications;
            Some(SettingChange::Notifications)
        }
        10 => {
            // Left and right both cycle; left goes one back, right one forward.
            s.replay_gain_mode = if step < 0 {
                s.replay_gain_mode.prev()
            } else {
                s.replay_gain_mode.cycle()
            };
            Some(SettingChange::ReplayGainMode)
        }
        11 => {
            let step_db = REPLAY_GAIN_PREAMP_STEP * f64::from(step);
            let new = (s.replay_gain_preamp + step_db).clamp(
                crate::config::REPLAY_GAIN_PREAMP_MIN,
                crate::config::REPLAY_GAIN_PREAMP_MAX,
            );
            // Skip the request/save round trip at the clamp boundary, same
            // as CavaSize/CoverArtSize above: comparing against the just-
            // clamped value (0.5 dB steps, exactly representable in f64),
            // not accumulated arithmetic, so exact equality is meaningful.
            #[allow(clippy::float_cmp)]
            if new == s.replay_gain_preamp {
                None
            } else {
                s.replay_gain_preamp = new;
                Some(SettingChange::ReplayGainPreamp)
            }
        }
        12 => {
            s.replay_gain_clip = !s.replay_gain_clip;
            Some(SettingChange::ReplayGainClip)
        }
        13 => {
            let cur = i32::from(s.playback_filters.min_rating);
            let new = crate::num::u8_sat((cur + step).clamp(0, 5));
            if new == s.playback_filters.min_rating {
                None
            } else {
                s.playback_filters.min_rating = new;
                Some(SettingChange::PlaybackFilters)
            }
        }
        14 => {
            let new = cycle_optional_year(s.playback_filters.year_min, step);
            if new == s.playback_filters.year_min {
                None
            } else {
                s.playback_filters.year_min = new;
                Some(SettingChange::PlaybackFilters)
            }
        }
        15 => {
            let new = cycle_optional_year(s.playback_filters.year_max, step);
            if new == s.playback_filters.year_max {
                None
            } else {
                s.playback_filters.year_max = new;
                Some(SettingChange::PlaybackFilters)
            }
        }
        16 => {
            let new = cycle_optional_duration(s.playback_filters.duration_min_secs, step);
            if new == s.playback_filters.duration_min_secs {
                None
            } else {
                s.playback_filters.duration_min_secs = new;
                Some(SettingChange::PlaybackFilters)
            }
        }
        17 => {
            let new = cycle_optional_duration(s.playback_filters.duration_max_secs, step);
            if new == s.playback_filters.duration_max_secs {
                None
            } else {
                s.playback_filters.duration_max_secs = new;
                Some(SettingChange::PlaybackFilters)
            }
        }
        _ => None,
    }
}

/// Cycle `Off -> YEAR_FILTER_RANGE.0 -> ... -> YEAR_FILTER_RANGE.1 -> Off`
/// (and the reverse on `step < 0`), `YEAR_FILTER_STEP` years per press.
const fn cycle_optional_year(cur: Option<i32>, step: i32) -> Option<i32> {
    let (min, max) = YEAR_FILTER_RANGE;
    let delta = step * YEAR_FILTER_STEP;
    let next = match cur {
        None if delta > 0 => min,
        None => max,
        Some(v) => v + delta,
    };
    if next < min || next > max {
        None
    } else {
        Some(next)
    }
}

/// Cycle `Off -> DURATION_FILTER_RANGE.0 -> ... -> DURATION_FILTER_RANGE.1 ->
/// Off` (and the reverse on `step < 0`), `DURATION_FILTER_STEP` seconds per press.
fn cycle_optional_duration(cur: Option<u32>, step: i32) -> Option<u32> {
    let (min, max) = DURATION_FILTER_RANGE;
    let delta = i64::from(step) * i64::from(DURATION_FILTER_STEP);
    let next = match cur {
        None if delta > 0 => i64::from(min),
        None => i64::from(max),
        Some(v) => i64::from(v) + delta,
    };
    if next < i64::from(min) || next > i64::from(max) {
        None
    } else {
        Some(crate::num::u32_sat(next))
    }
}

fn change_message(
    s: &crate::app::state::SettingsState,
    change: SettingChange,
    field: usize,
) -> String {
    match change {
        SettingChange::Theme => format!("Theme: {}", s.theme_name()),
        SettingChange::Cava => format!("Cava: {}", on_off(s.cava_enabled)),
        SettingChange::CavaSize => format!("Cava Size: {}%", s.cava_size),
        SettingChange::CoverArt => format!("Cover Art: {}", on_off(s.cover_art)),
        SettingChange::CoverArtSize => format!("Cover Art Size: {} rows", s.cover_art_size),
        SettingChange::Repeat => format!("Repeat: {}", s.repeat_mode.label()),
        SettingChange::AutoContinue => format!("Auto-continue: {}", on_off(s.auto_continue)),
        SettingChange::Scrobble => format!("Scrobble: {}", on_off(s.scrobble)),
        SettingChange::Daemon => format!("Daemon: {} (restart to apply)", on_off(s.daemon_enabled)),
        SettingChange::Notifications => format!("Notifications: {}", on_off(s.notifications)),
        SettingChange::ReplayGainMode => {
            format!("ReplayGain Mode: {}", s.replay_gain_mode.label())
        }
        SettingChange::ReplayGainPreamp => {
            format!(
                "ReplayGain Preamp: {}",
                crate::ui::pages::settings::format_preamp_db(s.replay_gain_preamp)
            )
        }
        SettingChange::ReplayGainClip => {
            format!(
                "ReplayGain Prevent Clipping: {}",
                on_off(s.replay_gain_clip)
            )
        }
        // One SettingChange variant covers all 5 filter fields (they share a
        // single SetPlaybackFilters wire request); `field` picks the message.
        SettingChange::PlaybackFilters => playback_filter_message(&s.playback_filters, field),
    }
}

fn format_optional(v: Option<impl std::fmt::Display>) -> String {
    v.map_or_else(|| "Off".to_string(), |v| v.to_string())
}

fn playback_filter_message(filters: &crate::config::PlaybackFilters, field: usize) -> String {
    match field {
        13 if filters.min_rating == 0 => "Min Rating: Off".to_string(),
        13 => format!("Min Rating: {}★ and below", filters.min_rating),
        14 => format!("Year Min: {}", format_optional(filters.year_min)),
        15 => format!("Year Max: {}", format_optional(filters.year_max)),
        16 => format!(
            "Duration Min: {}",
            format_optional(filters.duration_min_secs.map(|s| format!("{s}s")))
        ),
        17 => format!(
            "Duration Max: {}",
            format_optional(filters.duration_max_secs.map(|s| format!("{s}s")))
        ),
        _ => "Playback Filters updated".to_string(),
    }
}

const fn on_off(v: bool) -> &'static str {
    if v {
        "On"
    } else {
        "Off"
    }
}

#[cfg(test)]
mod tests {
    use super::{cycle_optional_duration, cycle_optional_year, YEAR_FILTER_RANGE};

    #[test]
    fn year_cycle_off_to_min_to_max_and_back_to_off() {
        assert_eq!(cycle_optional_year(None, 1), Some(YEAR_FILTER_RANGE.0));
        assert_eq!(
            cycle_optional_year(Some(YEAR_FILTER_RANGE.0), 1),
            Some(YEAR_FILTER_RANGE.0 + 5)
        );
        assert_eq!(cycle_optional_year(Some(YEAR_FILTER_RANGE.1), 1), None);
    }

    #[test]
    fn year_cycle_reverse_off_to_max_to_min_and_back_to_off() {
        assert_eq!(cycle_optional_year(None, -1), Some(YEAR_FILTER_RANGE.1));
        assert_eq!(
            cycle_optional_year(Some(YEAR_FILTER_RANGE.1), -1),
            Some(YEAR_FILTER_RANGE.1 - 5)
        );
        assert_eq!(cycle_optional_year(Some(YEAR_FILTER_RANGE.0), -1), None);
    }

    #[test]
    fn duration_cycle_off_to_zero_and_back_to_off() {
        assert_eq!(cycle_optional_duration(None, 1), Some(0));
        assert_eq!(cycle_optional_duration(Some(0), -1), None);
    }

    #[test]
    fn duration_cycle_never_underflows_below_zero() {
        // A negative step at the floor must land on Off, not wrap/panic.
        assert_eq!(cycle_optional_duration(Some(0), -1), None);
    }

    #[test]
    fn duration_cycle_caps_at_the_configured_max() {
        assert_eq!(cycle_optional_duration(Some(3600), 1), None);
    }
}
