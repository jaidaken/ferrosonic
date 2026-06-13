//! Quit-confirm modal: on quit, ask whether to stop the background daemon.

use ratatui::{
    layout::{Alignment, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

use crate::ui::theme::ThemeColors;

/// Draw a centered "stop the daemon?" confirmation over the frame.
pub fn render(frame: &mut Frame<'_>, area: Rect, colors: &ThemeColors) {
    let w = 56.min(area.width);
    let h = 7.min(area.height);
    if w < 4 || h < 4 {
        return;
    }
    let x = area.x + area.width.saturating_sub(w) / 2;
    let y = area.y + area.height.saturating_sub(h) / 2;
    let rect = Rect::new(x, y, w, h);

    let key = |s: &str| {
        Span::styled(
            s.to_string(),
            Style::default()
                .fg(colors.primary)
                .add_modifier(Modifier::BOLD),
        )
    };

    let lines = vec![
        Line::from(""),
        Line::from("Stop the background daemon too?"),
        Line::from(""),
        Line::from(vec![
            key("[Y]"),
            Span::raw(" Stop daemon   "),
            key("[N]"),
            Span::raw(" Keep playing   "),
            Span::styled("[Esc]", Style::default().fg(colors.muted)),
            Span::raw(" Cancel"),
        ]),
    ];

    let para = Paragraph::new(lines)
        .alignment(Alignment::Center)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(colors.accent))
                .title(" Quit Ferrosonic "),
        );

    frame.render_widget(Clear, rect);
    frame.render_widget(para, rect);
}
