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

/// Render the Settings page.
pub fn render(frame: &mut Frame<'_>, area: Rect, state: &AppState<'_>) {
    let colors = *state.client.settings_state.theme_colors();

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Settings ")
        .border_style(Style::default().fg(colors.border_focused));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height < 18 {
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
    let daemon_val = if s.daemon_enabled { "On" } else { "Off" }.to_string();

    let mut y = inner.y;
    let x = inner.x;
    let w = inner.width;

    let buf = frame.buffer_mut();

    section_heading(buf, Rect::new(x, y, w, 1), "Display", &colors);
    y += 1;
    setting_row(
        buf,
        Rect::new(x, y, w, 1),
        "Theme",
        &theme_val,
        sel == 0,
        &colors,
    );
    y += 2;

    section_heading(buf, Rect::new(x, y, w, 1), "Now Playing", &colors);
    y += 1;
    setting_row(
        buf,
        Rect::new(x, y, w, 1),
        "Cava Visualizer",
        &cava_val,
        sel == 1,
        &colors,
    );
    y += 1;
    setting_row(
        buf,
        Rect::new(x, y, w, 1),
        "Cava Size",
        &cava_size_val,
        sel == 2,
        &colors,
    );
    y += 1;
    setting_row(
        buf,
        Rect::new(x, y, w, 1),
        "Cover Art",
        &cover_val,
        sel == 3,
        &colors,
    );
    y += 1;
    setting_row(
        buf,
        Rect::new(x, y, w, 1),
        "Cover Art Size",
        &cover_size_val,
        sel == 4,
        &colors,
    );
    y += 2;

    section_heading(buf, Rect::new(x, y, w, 1), "Playback", &colors);
    y += 1;
    setting_row(
        buf,
        Rect::new(x, y, w, 1),
        "Repeat",
        &repeat_val,
        sel == 5,
        &colors,
    );
    y += 1;
    setting_row(
        buf,
        Rect::new(x, y, w, 1),
        "Auto-continue",
        &auto_val,
        sel == 6,
        &colors,
    );
    y += 2;

    section_heading(buf, Rect::new(x, y, w, 1), "System", &colors);
    y += 1;
    setting_row(
        buf,
        Rect::new(x, y, w, 1),
        "Daemon",
        &daemon_val,
        sel == 7,
        &colors,
    );

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
