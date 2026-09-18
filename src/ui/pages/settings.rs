//! Settings page.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Widget},
    Frame,
};

use crate::app::state::AppState;
use crate::ui::theme::ThemeColors;

/// One row of the settings list; `Gap` is a blank spacer line.
enum Item {
    Heading(&'static str),
    Row {
        label: &'static str,
        value: String,
        idx: usize,
    },
    Gap,
}

/// Render the Settings page.
// Cohesive single match/render; splitting would fragment one logical unit.
#[allow(clippy::too_many_lines)]
pub fn render(frame: &mut Frame<'_>, area: Rect, state: &AppState<'_>) {
    let colors = *state.client.settings_state.theme_colors();

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Settings ")
        .border_style(Style::default().fg(colors.border_focused));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height == 0 {
        return;
    }

    let settings = &state.client.settings_state;
    let cava_ok = state.client.cava_available;
    let sel = settings.selected_field;

    let theme_val = settings.theme_name().to_string();
    let cava_val = if !cava_ok {
        "Off (cava not found)".to_string()
    } else if settings.cava_enabled {
        "On".into()
    } else {
        "Off".into()
    };
    let cava_size_val = if cava_ok {
        format!("{}%", settings.cava_size)
    } else {
        "N/A".into()
    };
    let cover_val = if settings.cover_art { "On" } else { "Off" }.to_string();
    let cover_size_val = format!("{} rows", settings.cover_art_size);
    let repeat_val = match settings.repeat_mode {
        crate::config::RepeatMode::Off => "Off",
        crate::config::RepeatMode::One => "One",
        crate::config::RepeatMode::All => "All",
    }
    .to_string();
    let auto_val = if settings.auto_continue { "On" } else { "Off" }.to_string();
    let stream_val = if settings.stream_on_start {
        "On"
    } else {
        "Off"
    }
    .to_string();
    let resume_val = if settings.resume_on_start {
        "On"
    } else {
        "Off"
    }
    .to_string();
    let autoplay_val = if settings.autoplay_on_start {
        "On"
    } else {
        "Off"
    }
    .to_string();
    let offline_val = if settings.offline_cache_enabled {
        "On"
    } else {
        "Off"
    }
    .to_string();
    let offline_size_val = format!("{} MB", settings.offline_cache_max_mb);
    let scrobble_val = if settings.scrobble { "On" } else { "Off" }.to_string();
    let daemon_val = if settings.daemon_enabled { "On" } else { "Off" }.to_string();
    let notifications_val = if settings.notifications { "On" } else { "Off" }.to_string();
    let rg_mode_val = settings.replay_gain_mode.label().to_string();
    let rg_preamp_val = format_preamp_db(settings.replay_gain_preamp);
    let rg_clip_val = if settings.replay_gain_clip {
        "On"
    } else {
        "Off"
    }
    .to_string();
    let filters = &settings.playback_filters;
    let min_rating_val = if filters.min_rating == 0 {
        "Off".to_string()
    } else {
        format!("{}★ and below", filters.min_rating)
    };
    let year_min_val = filters
        .year_min
        .map_or_else(|| "Off".to_string(), |y| y.to_string());
    let year_max_val = filters
        .year_max
        .map_or_else(|| "Off".to_string(), |y| y.to_string());
    let duration_min_val = filters
        .duration_min_secs
        .map_or_else(|| "Off".to_string(), |s| format!("{s}s"));
    let duration_max_val = filters
        .duration_max_secs
        .map_or_else(|| "Off".to_string(), |s| format!("{s}s"));
    let excluded_genres_val = format!("{} entries", filters.excluded_genres.len());
    let excluded_artists_val = format!("{} entries", filters.excluded_artists.len());
    let keybindings_val = if settings.keybindings.is_empty() {
        "Defaults".to_string()
    } else {
        format!("{} overrides", settings.keybindings.len())
    };
    let x = inner.x;
    let w = inner.width;

    // Ordered rows; Gap is a blank spacer. Rendered top-down within the area,
    // stopping before the help line, so a short area degrades, not blanks.
    let items = [
        Item::Heading("Display"),
        Item::Row {
            label: "Theme",
            value: theme_val,
            idx: 0,
        },
        Item::Gap,
        Item::Heading("Now Playing"),
        Item::Row {
            label: "Cava Visualizer",
            value: cava_val,
            idx: 1,
        },
        Item::Row {
            label: "Cava Size",
            value: cava_size_val,
            idx: 2,
        },
        Item::Row {
            label: "Cover Art",
            value: cover_val,
            idx: 3,
        },
        Item::Row {
            label: "Cover Art Size",
            value: cover_size_val,
            idx: 4,
        },
        Item::Gap,
        Item::Heading("Playback"),
        Item::Row {
            label: "Repeat",
            value: repeat_val,
            idx: 5,
        },
        Item::Row {
            label: "Auto-continue",
            value: auto_val,
            idx: 6,
        },
        Item::Row {
            label: "Stream on Start",
            value: stream_val,
            idx: 7,
        },
        Item::Row {
            label: "Scrobble",
            value: scrobble_val,
            idx: 8,
        },
        Item::Gap,
        Item::Heading("System"),
        Item::Row {
            label: "Daemon",
            value: daemon_val,
            idx: 9,
        },
        Item::Row {
            label: "Notifications",
            value: notifications_val,
            idx: 10,
        },
        Item::Gap,
        Item::Heading("ReplayGain"),
        Item::Row {
            label: "Mode",
            value: rg_mode_val,
            idx: 11,
        },
        Item::Row {
            label: "Preamp",
            value: rg_preamp_val,
            idx: 12,
        },
        Item::Row {
            label: "Prevent Clipping",
            value: rg_clip_val,
            idx: 13,
        },
        Item::Gap,
        Item::Heading("Playback Filters"),
        Item::Row {
            label: "Min Rating",
            value: min_rating_val,
            idx: 14,
        },
        Item::Row {
            label: "Year Min",
            value: year_min_val,
            idx: 15,
        },
        Item::Row {
            label: "Year Max",
            value: year_max_val,
            idx: 16,
        },
        Item::Row {
            label: "Duration Min",
            value: duration_min_val,
            idx: 17,
        },
        Item::Row {
            label: "Duration Max",
            value: duration_max_val,
            idx: 18,
        },
        Item::Row {
            label: "Excluded Genres",
            value: excluded_genres_val,
            idx: 19,
        },
        Item::Row {
            label: "Excluded Artists",
            value: excluded_artists_val,
            idx: 20,
        },
        Item::Gap,
        Item::Heading("Controls"),
        Item::Row {
            label: "Global Keybinds",
            value: keybindings_val,
            idx: 21,
        },
        Item::Gap,
        Item::Heading("Startup"),
        Item::Row {
            label: "Resume on Start",
            value: resume_val,
            idx: 22,
        },
        Item::Row {
            label: "Autoplay on Start",
            value: autoplay_val,
            idx: 23,
        },
        Item::Row {
            label: "Offline Cache",
            value: offline_val,
            idx: 24,
        },
        Item::Row {
            label: "Offline Cache Size",
            value: offline_size_val,
            idx: 25,
        },
    ];

    {
        let row_limit = inner.y + inner.height.saturating_sub(1);
        // Scroll so the selected row stays visible when the panel is too short.
        let visible = (row_limit - inner.y) as usize;
        let sel_idx = items
            .iter()
            .position(|it| matches!(it, Item::Row { idx, .. } if *idx == sel))
            .unwrap_or(0);
        let start = sel_idx.saturating_sub(visible.saturating_sub(1));
        let buf = frame.buffer_mut();
        for (y, item) in (inner.y..).zip(items.iter().skip(start)) {
            if y >= row_limit {
                break;
            }
            match item {
                Item::Heading(h) => section_heading(buf, Rect::new(x, y, w, 1), h, &colors),
                Item::Row { label, value, idx } => {
                    setting_row(
                        buf,
                        Rect::new(x, y, w, 1),
                        label,
                        value,
                        sel == *idx,
                        &colors,
                    );
                }
                Item::Gap => {}
            }
        }
    }

    let help_text = settings_help_text(sel, cava_ok);
    let help_y = inner.y + inner.height.saturating_sub(1);
    let help = Paragraph::new(help_text).style(Style::default().fg(colors.muted));
    help.render(
        Rect::new(inner.x, help_y, inner.width, 1),
        frame.buffer_mut(),
    );
}

/// Help-line text for the selected settings field. Indices MUST track the
/// `Item::Row { idx }` order in `render`: 7 Stream on Start, 8 Scrobble, 9
/// Daemon, 10 Notifications, 11 `ReplayGain` Mode, 12 `ReplayGain` Preamp,
/// 13 `ReplayGain` Prevent Clipping, 14 Min Rating, 15 Year Min, 16 Year Max,
/// 17 Duration Min, 18 Duration Max, 19 Excluded Genres, 20 Excluded Artists,
/// 21 Global Keybinds, 22 Resume on Start, 23 Autoplay on Start, 24 Offline
/// Cache, 25 Offline Cache Size.
// Each setting's cava-ok/not-installed cases kept adjacent; merging would split setting 2.
#[allow(clippy::match_same_arms)]
const fn settings_help_text(sel: usize, cava_ok: bool) -> &'static str {
    match sel {
        0 => "← → or Enter to change theme (auto-saves)",
        1 if cava_ok => "← → or Enter to toggle cava visualizer (auto-saves)",
        1 => "cava is not installed on this system",
        2 if cava_ok => "← → to adjust cava size (10%-80%, step 5)",
        2 => "cava is not installed on this system",
        3 => "← → or Enter to toggle cover art in the now-playing section",
        4 => "← → to adjust now-playing height when art is visible (8-24 rows, step 2)",
        5 => "← → or Enter to cycle repeat mode (off / one / all)",
        6 => "← → or Enter to toggle auto-continue (random songs when queue ends)",
        7 => {
            "← → or Enter to stream on start (play immediately) or pre-buffer the whole track first"
        }
        8 => "← → or Enter to toggle scrobbling (report plays to the server)",
        9 => "← → or Enter to toggle background daemon (takes effect on next launch)",
        10 => "← → or Enter to toggle desktop notifications on track change",
        11 => "← → or Enter to cycle ReplayGain mode (off / track / album); applies live",
        12 => "← → to adjust ReplayGain preamp (-15.0 to 15.0 dB, step 0.5); applies live",
        13 => "← → or Enter to toggle clip prevention; applies live",
        14 => "← → to exclude songs rated at or below this (0 = off, step 1)",
        15 => "← → to exclude songs released before this year (off = no lower bound)",
        16 => "← → to exclude songs released after this year (off = no upper bound)",
        17 => "← → to exclude songs shorter than this (off = no lower bound, step 15s)",
        18 => "← → to exclude songs longer than this (off = no upper bound, step 15s)",
        19 => "Enter to edit excluded genres",
        20 => "Enter to edit excluded artists",
        21 => "Enter to edit global keybindings",
        22 => "← → or Enter to restore the queue, track, and playhead on the next daemon start",
        23 => "← → or Enter to auto-play a restored session instead of restoring it paused",
        24 => "← → or Enter to cache streamed tracks locally for repeat/offline playback",
        25 => "← → to set the offline cache size cap (MB, step 512)",
        _ => "",
    }
}

/// Format a `ReplayGain` preamp value for display, e.g. `+1.5 dB`, `-2.0 dB`, `0.0 dB`.
/// Shared with `app::input_settings`'s change-notification toast so the row
/// value and the toast text can never drift apart.
pub(crate) fn format_preamp_db(db: f64) -> String {
    if db > 0.0 {
        format!("+{db:.1} dB")
    } else {
        format!("{db:.1} dB")
    }
}

fn section_heading(buf: &mut Buffer, area: Rect, label: &str, colors: &ThemeColors) {
    let line = Line::from(vec![Span::styled(
        label.to_string(),
        Style::default()
            .fg(colors.accent)
            .add_modifier(Modifier::BOLD),
    )]);
    Paragraph::new(line).render(area, buf);
}

const LABEL_COL_WIDTH: u16 = 18;

fn setting_row(
    buf: &mut Buffer,
    area: Rect,
    label: &str,
    value: &str,
    selected: bool,
    colors: &ThemeColors,
) {
    let (marker, label_style, value_style) = if selected {
        (
            "▶ ",
            Style::default()
                .fg(colors.primary)
                .add_modifier(Modifier::BOLD),
            Style::default().fg(colors.accent),
        )
    } else {
        (
            "  ",
            Style::default().fg(colors.highlight_fg),
            Style::default().fg(colors.muted),
        )
    };

    // Pad label to LABEL_COL_WIDTH so values line up across rows.
    let padded_label = format!("{:<width$}", label, width = LABEL_COL_WIDTH as usize);

    let arrows = if selected {
        Span::styled("  ◀ ▶", Style::default().fg(colors.muted))
    } else {
        Span::raw("")
    };

    let line = Line::from(vec![
        Span::styled(marker.to_string(), label_style),
        Span::styled(padded_label, label_style),
        Span::styled(value.to_string(), value_style),
        arrows,
    ]);
    Paragraph::new(line).render(area, buf);
}

#[cfg(test)]
mod tests {
    use super::{format_preamp_db, settings_help_text};

    #[test]
    fn help_text_tracks_each_field_index() {
        assert!(
            settings_help_text(6, true).contains("auto-continue"),
            "idx 6 is Auto-continue"
        );
        assert!(
            settings_help_text(7, true).contains("stream on start"),
            "idx 7 is Stream on Start"
        );
        assert!(
            settings_help_text(8, true).contains("scrobbling"),
            "idx 8 is Scrobble, not the daemon row"
        );
        assert!(
            settings_help_text(9, true).contains("daemon"),
            "idx 9 is Daemon"
        );
        assert!(
            settings_help_text(10, true).contains("notifications"),
            "idx 10 is Desktop Notifications"
        );
        assert!(
            settings_help_text(11, true).contains("ReplayGain mode"),
            "idx 11 is ReplayGain Mode"
        );
        assert!(
            settings_help_text(12, true).contains("preamp"),
            "idx 12 is ReplayGain Preamp"
        );
        assert!(
            settings_help_text(13, true).contains("clip prevention"),
            "idx 13 is ReplayGain Prevent Clipping"
        );
        assert!(
            settings_help_text(14, true).contains("rated at or below"),
            "idx 14 is Min Rating"
        );
        assert!(
            settings_help_text(15, true).contains("before this year"),
            "idx 15 is Year Min"
        );
        assert!(
            settings_help_text(16, true).contains("after this year"),
            "idx 16 is Year Max"
        );
        assert!(
            settings_help_text(17, true).contains("shorter than"),
            "idx 17 is Duration Min"
        );
        assert!(
            settings_help_text(18, true).contains("longer than"),
            "idx 18 is Duration Max"
        );
        assert!(
            settings_help_text(22, true).contains("restore the queue"),
            "idx 22 is Resume on Start"
        );
        assert!(
            settings_help_text(24, true).contains("cache streamed tracks"),
            "idx 24 is Offline Cache"
        );
        assert_eq!(
            settings_help_text(26, true),
            "",
            "no field beyond Offline Cache Size"
        );
    }

    #[test]
    fn help_text_gates_cava_rows_on_availability() {
        assert!(settings_help_text(1, true).contains("cava visualizer"));
        assert!(settings_help_text(1, false).contains("not installed"));
        assert!(settings_help_text(2, false).contains("not installed"));
    }

    #[test]
    fn preamp_formatting_signs_correctly() {
        assert_eq!(format_preamp_db(1.5), "+1.5 dB");
        assert_eq!(format_preamp_db(-2.0), "-2.0 dB");
        assert_eq!(format_preamp_db(0.0), "0.0 dB");
    }
}
