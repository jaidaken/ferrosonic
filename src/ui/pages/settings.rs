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

    let s = &state.client.settings_state;
    let cava_ok = state.client.cava_available;
    let sel = s.selected_field;

    let theme_val = s.theme_name().to_string();
    let cava_val = if !cava_ok {
        "Off (cava not found)".to_string()
    } else if s.cava_enabled {
        "On".into()
    } else {
        "Off".into()
    };
    let cava_size_val = if !cava_ok {
        "N/A".into()
    } else {
        format!("{}%", s.cava_size)
    };
    let cover_val = if s.cover_art { "On" } else { "Off" }.to_string();
    let cover_size_val = format!("{} rows", s.cover_art_size);
    let repeat_val = match s.repeat_mode {
        crate::config::RepeatMode::Off => "Off",
        crate::config::RepeatMode::One => "One",
        crate::config::RepeatMode::All => "All",
    }
    .to_string();
    let auto_val = if s.auto_continue { "On" } else { "Off" }.to_string();
    let scrobble_val = if s.scrobble { "On" } else { "Off" }.to_string();
    let daemon_val = if s.daemon_enabled { "On" } else { "Off" }.to_string();

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
            label: "Scrobble",
            value: scrobble_val,
            idx: 7,
        },
        Item::Gap,
        Item::Heading("System"),
        Item::Row {
            label: "Daemon",
            value: daemon_val,
            idx: 8,
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
        let mut y = inner.y;
        for item in items.iter().skip(start) {
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
            y += 1;
        }
    }

    let help_text = match sel {
        0 => "← → or Enter to change theme (auto-saves)",
        1 if cava_ok => "← → or Enter to toggle cava visualizer (auto-saves)",
        1 => "cava is not installed on this system",
        2 if cava_ok => "← → to adjust cava size (10%-80%, step 5)",
        2 => "cava is not installed on this system",
        3 => "← → or Enter to toggle cover art in the now-playing section",
        4 => "← → to adjust now-playing height when art is visible (8-24 rows, step 2)",
        5 => "← → or Enter to cycle repeat mode (off / one / all)",
        6 => "← → or Enter to toggle auto-continue (random songs when queue ends)",
        7 => "← → or Enter to toggle background daemon (takes effect on next launch)",
        _ => "",
    };
    let help_y = inner.y + inner.height.saturating_sub(1);
    let help = Paragraph::new(help_text).style(Style::default().fg(colors.muted));
    help.render(
        Rect::new(inner.x, help_y, inner.width, 1),
        frame.buffer_mut(),
    );
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
