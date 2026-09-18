use crossterm::event::{self, KeyCode, KeyModifiers};

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
    StreamOnStart,
    ResumeOnStart,
    AutoplayOnStart,
    OfflineCache,
    OfflineCacheSize,
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

const SETTINGS_FIELD_COUNT: usize = 26;
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
                KeyCode::Enter if (19..=21).contains(&field) => match field {
                    19 | 20 => {
                        let kind = if field == 19 {
                            crate::app::state::FilterListKind::Genres
                        } else {
                            crate::app::state::FilterListKind::Artists
                        };
                        let entries = if field == 19 {
                            cs.settings_state.playback_filters.excluded_genres.clone()
                        } else {
                            cs.settings_state.playback_filters.excluded_artists.clone()
                        };
                        cs.settings_state.filter_editor =
                            Some(crate::app::state::FilterListEditor {
                                kind,
                                entries,
                                selected: 0,
                                adding: false,
                                input: String::new(),
                            });
                    }
                    21 => {
                        cs.settings_state.keybinding_editor =
                            Some(crate::app::state::KeybindingEditor {
                                bindings: cs.settings_state.keybindings.clone(),
                                selected: 0,
                                capturing: false,
                            });
                    }
                    _ => {}
                },
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
            stream_on_start,
            resume_on_start,
            autoplay_on_start,
            offline_cache_enabled,
            offline_cache_max_mb,
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
                s.stream_on_start,
                s.resume_on_start,
                s.autoplay_on_start,
                s.offline_cache_enabled,
                s.offline_cache_max_mb,
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
            SettingChange::StreamOnStart => DaemonRequest::SetStreamOnStart(stream_on_start),
            SettingChange::ResumeOnStart => DaemonRequest::SetResumeOnStart(resume_on_start),
            SettingChange::AutoplayOnStart => DaemonRequest::SetAutoplayOnStart(autoplay_on_start),
            SettingChange::OfflineCache => {
                DaemonRequest::SetOfflineCacheEnabled(offline_cache_enabled)
            }
            SettingChange::OfflineCacheSize => {
                DaemonRequest::SetOfflineCacheMaxMb(offline_cache_max_mb)
            }
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
            | SettingChange::StreamOnStart
            | SettingChange::ResumeOnStart
            | SettingChange::AutoplayOnStart
            | SettingChange::OfflineCache
            | SettingChange::OfflineCacheSize
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

    /// Handle either Settings editor overlay. Edits stay local until Ctrl+S.
    pub(super) async fn handle_settings_editor_key(
        &mut self,
        key: event::KeyEvent,
    ) -> Result<(), Error> {
        if self
            .client_state
            .read()
            .await
            .settings_state
            .filter_editor
            .is_some()
        {
            return self.handle_filter_editor_key(key).await;
        }
        self.handle_keybinding_editor_key(key).await
    }

    // One modal state machine; keeping key handling together makes add/list/save
    // ownership explicit.
    #[allow(clippy::too_many_lines, clippy::significant_drop_tightening)]
    async fn handle_filter_editor_key(&self, key: event::KeyEvent) -> Result<(), Error> {
        let save = key.code == KeyCode::Char('s') && key.modifiers.contains(KeyModifiers::CONTROL);
        if save {
            let (kind, entries, mut filters) = {
                let cs = self.client_state.read().await;
                let Some(editor) = cs.settings_state.filter_editor.as_ref() else {
                    return Ok(());
                };
                (
                    editor.kind,
                    editor.entries.clone(),
                    cs.settings_state.playback_filters.clone(),
                )
            };
            match kind {
                crate::app::state::FilterListKind::Genres => {
                    filters.excluded_genres.clone_from(&entries);
                }
                crate::app::state::FilterListKind::Artists => {
                    filters.excluded_artists.clone_from(&entries);
                }
            }
            if let Err(error) = self
                .client
                .request(DaemonRequest::SetPlaybackFilters(filters.clone()))
                .await
            {
                self.client_state
                    .write()
                    .await
                    .notify_error(format!("Failed to save filters: {error}"));
                return Ok(());
            }
            let mut cs = self.client_state.write().await;
            cs.settings_state.playback_filters = filters;
            cs.settings_state.filter_editor = None;
            cs.notify("Playback filters saved");
            return Ok(());
        }

        let mut cs = self.client_state.write().await;
        let mut notice = None;
        let Some(editor) = cs.settings_state.filter_editor.as_mut() else {
            return Ok(());
        };
        if editor.adding {
            match key.code {
                KeyCode::Esc => {
                    editor.adding = false;
                    editor.input.clear();
                }
                KeyCode::Backspace => {
                    editor.input.pop();
                }
                KeyCode::Enter => {
                    let value = editor.input.trim().to_string();
                    if value.is_empty() {
                        notice = Some((true, "Exclusion cannot be empty".to_string()));
                    } else if editor
                        .entries
                        .iter()
                        .any(|entry| entry.eq_ignore_ascii_case(&value))
                    {
                        notice = Some((true, format!("'{value}' is already excluded")));
                    } else {
                        editor.entries.push(value);
                        editor.selected = editor.entries.len().saturating_sub(1);
                        editor.adding = false;
                        editor.input.clear();
                    }
                }
                KeyCode::Char(character)
                    if !key
                        .modifiers
                        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
                {
                    editor.input.push(character);
                }
                _ => {}
            }
        } else {
            match key.code {
                KeyCode::Esc => cs.settings_state.filter_editor = None,
                KeyCode::Up | KeyCode::Char('k') => {
                    editor.selected = editor.selected.saturating_sub(1);
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    if editor.selected + 1 < editor.entries.len() {
                        editor.selected += 1;
                    }
                }
                KeyCode::Char('a') => {
                    editor.adding = true;
                    editor.input.clear();
                }
                KeyCode::Char('d') if !editor.entries.is_empty() => {
                    editor
                        .entries
                        .remove(editor.selected.min(editor.entries.len() - 1));
                    editor.selected = editor.selected.min(editor.entries.len().saturating_sub(1));
                }
                _ => {}
            }
        }
        if let Some((is_error, message)) = notice {
            if is_error {
                cs.notify_error(message);
            }
        }
        Ok(())
    }

    #[allow(clippy::significant_drop_tightening)]
    async fn handle_keybinding_editor_key(&mut self, key: event::KeyEvent) -> Result<(), Error> {
        use crate::config::keybind::{resolve, KeyChord, DEFAULT_BINDINGS};

        let capturing = self
            .client_state
            .read()
            .await
            .settings_state
            .keybinding_editor
            .as_ref()
            .is_some_and(|editor| editor.capturing);
        let save = !capturing
            && key.code == KeyCode::Char('s')
            && key.modifiers.contains(KeyModifiers::CONTROL);
        if save {
            let bindings = {
                let cs = self.client_state.read().await;
                let Some(editor) = cs.settings_state.keybinding_editor.as_ref() else {
                    return Ok(());
                };
                editor.bindings.clone()
            };
            let (keymap, warnings) = resolve(&bindings);
            if let Some(warning) = warnings.first() {
                self.client_state
                    .write()
                    .await
                    .notify_error(warning.clone());
                return Ok(());
            }
            if let Err(error) = self
                .client
                .request(DaemonRequest::SetKeybindings(bindings.clone()))
                .await
            {
                self.client_state
                    .write()
                    .await
                    .notify_error(format!("Failed to save keybindings: {error}"));
                return Ok(());
            }
            self.keymap = keymap;
            let mut cs = self.client_state.write().await;
            cs.settings_state.keybindings = bindings;
            cs.settings_state.keybinding_editor = None;
            cs.notify("Keybindings saved and applied");
            return Ok(());
        }

        let mut cs = self.client_state.write().await;
        let mut notice = None;
        let Some(editor) = cs.settings_state.keybinding_editor.as_mut() else {
            return Ok(());
        };
        if editor.capturing {
            if key.code == KeyCode::Esc {
                editor.capturing = false;
            } else {
                let (action, _) = DEFAULT_BINDINGS[editor.selected];
                let chord = KeyChord::from(key);
                let mut candidate = editor.bindings.clone();
                candidate.insert(action, chord);
                let (resolved, warnings) = resolve(&candidate);
                if warnings.is_empty() && resolved.get(&chord) == Some(&action) {
                    editor.bindings = candidate;
                    editor.capturing = false;
                } else {
                    notice = Some(warnings.first().cloned().unwrap_or_else(|| {
                        format!("{chord} cannot be used for {}", action.label())
                    }));
                }
            }
        } else {
            match key.code {
                KeyCode::Esc => cs.settings_state.keybinding_editor = None,
                KeyCode::Up | KeyCode::Char('k') => {
                    editor.selected = editor.selected.saturating_sub(1);
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    editor.selected = (editor.selected + 1).min(DEFAULT_BINDINGS.len() - 1);
                }
                KeyCode::Enter => editor.capturing = true,
                KeyCode::Char('d') => {
                    editor.bindings.remove(&DEFAULT_BINDINGS[editor.selected].0);
                }
                KeyCode::Char('D') => editor.bindings.clear(),
                _ => {}
            }
        }
        if let Some(message) = notice {
            cs.notify_error(message);
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
            s.stream_on_start = !s.stream_on_start;
            Some(SettingChange::StreamOnStart)
        }
        8 => {
            s.scrobble = !s.scrobble;
            Some(SettingChange::Scrobble)
        }
        9 => {
            s.daemon_enabled = !s.daemon_enabled;
            Some(SettingChange::Daemon)
        }
        10 => {
            s.notifications = !s.notifications;
            Some(SettingChange::Notifications)
        }
        11 => {
            // Left and right both cycle; left goes one back, right one forward.
            s.replay_gain_mode = if step < 0 {
                s.replay_gain_mode.prev()
            } else {
                s.replay_gain_mode.cycle()
            };
            Some(SettingChange::ReplayGainMode)
        }
        12 => {
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
        13 => {
            s.replay_gain_clip = !s.replay_gain_clip;
            Some(SettingChange::ReplayGainClip)
        }
        14 => {
            let cur = i32::from(s.playback_filters.min_rating);
            let new = crate::num::u8_sat((cur + step).clamp(0, 5));
            if new == s.playback_filters.min_rating {
                None
            } else {
                s.playback_filters.min_rating = new;
                Some(SettingChange::PlaybackFilters)
            }
        }
        15 => {
            let new = cycle_optional_year(s.playback_filters.year_min, step);
            if new == s.playback_filters.year_min {
                None
            } else {
                s.playback_filters.year_min = new;
                Some(SettingChange::PlaybackFilters)
            }
        }
        16 => {
            let new = cycle_optional_year(s.playback_filters.year_max, step);
            if new == s.playback_filters.year_max {
                None
            } else {
                s.playback_filters.year_max = new;
                Some(SettingChange::PlaybackFilters)
            }
        }
        17 => {
            let new = cycle_optional_duration(s.playback_filters.duration_min_secs, step);
            if new == s.playback_filters.duration_min_secs {
                None
            } else {
                s.playback_filters.duration_min_secs = new;
                Some(SettingChange::PlaybackFilters)
            }
        }
        18 => {
            let new = cycle_optional_duration(s.playback_filters.duration_max_secs, step);
            if new == s.playback_filters.duration_max_secs {
                None
            } else {
                s.playback_filters.duration_max_secs = new;
                Some(SettingChange::PlaybackFilters)
            }
        }
        22 => {
            s.resume_on_start = !s.resume_on_start;
            Some(SettingChange::ResumeOnStart)
        }
        23 => {
            s.autoplay_on_start = !s.autoplay_on_start;
            Some(SettingChange::AutoplayOnStart)
        }
        24 => {
            s.offline_cache_enabled = !s.offline_cache_enabled;
            Some(SettingChange::OfflineCache)
        }
        25 => {
            let cur = i64::from(s.offline_cache_max_mb);
            let step_mb = i64::from(step) * 512;
            let new = (cur + step_mb).clamp(1, 102_400);
            let new = crate::num::u32_sat(new);
            if new == s.offline_cache_max_mb {
                None
            } else {
                s.offline_cache_max_mb = new;
                Some(SettingChange::OfflineCacheSize)
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
        SettingChange::StreamOnStart => format!("Stream on start: {}", on_off(s.stream_on_start)),
        SettingChange::ResumeOnStart => format!("Resume on start: {}", on_off(s.resume_on_start)),
        SettingChange::AutoplayOnStart => {
            format!("Autoplay on start: {}", on_off(s.autoplay_on_start))
        }
        SettingChange::OfflineCache => {
            format!("Offline cache: {}", on_off(s.offline_cache_enabled))
        }
        SettingChange::OfflineCacheSize => {
            format!("Offline cache size: {} MB", s.offline_cache_max_mb)
        }
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
        14 if filters.min_rating == 0 => "Min Rating: Off".to_string(),
        14 => format!("Min Rating: {}★ and below", filters.min_rating),
        15 => format!("Year Min: {}", format_optional(filters.year_min)),
        16 => format!("Year Max: {}", format_optional(filters.year_max)),
        17 => format!(
            "Duration Min: {}",
            format_optional(filters.duration_min_secs.map(|s| format!("{s}s")))
        ),
        18 => format!(
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
