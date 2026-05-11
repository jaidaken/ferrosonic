//! Footer keybind hints + status.

use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::Widget,
};

use crate::app::state::{Notification, Page};
use crate::ui::theme::ThemeColors;

pub struct Footer<'a> {
    page: Page,
    sample_rate: Option<u32>,
    notification: Option<&'a Notification>,
    repeat_mode: crate::config::RepeatMode,
    colors: ThemeColors,
}

impl<'a> Footer<'a> {
    pub fn new(page: Page, colors: ThemeColors) -> Self {
        Self {
            page,
            sample_rate: None,
            notification: None,
            repeat_mode: crate::config::RepeatMode::Off,
            colors,
        }
    }

    pub fn sample_rate(mut self, rate: Option<u32>) -> Self {
        self.sample_rate = rate;
        self
    }

    pub fn notification(mut self, notification: Option<&'a Notification>) -> Self {
        self.notification = notification;
        self
    }

    pub fn repeat_mode(mut self, mode: crate::config::RepeatMode) -> Self {
        self.repeat_mode = mode;
        self
    }

    fn global_keybinds(&self) -> Vec<(String, String)> {
        let repeat_label = format!("Repeat ({})", self.repeat_mode.label());
        vec![
            ("q".into(), "Quit".into()),
            ("p/Space".into(), "Pause".into()),
            ("h".into(), "Prev".into()),
            ("l".into(), "Next".into()),
            ("n".into(), "Star playing".into()),
            ("r".into(), repeat_label),
            ("Shift+T".into(), "Shuffle library".into()),
        ]
    }

    fn page_keybinds(&self) -> Vec<(String, String)> {
        let s = |k: &str, d: &str| (k.to_string(), d.to_string());
        match self.page {
            Page::QuickPlay => vec![s("m", "Star selected"), s("Enter", "Play")],
            Page::Library => vec![
                s("m", "Star selected"),
                s("/", "Filter"),
                s("←/→", "Focus"),
                s("e", "Add"),
                s("i", "Add next"),
                s("t", "Shuffle"),
                s("Enter", "Play"),
            ],
            Page::Queue => vec![
                s("m", "Star selected"),
                s("d", "Remove"),
                s("J/K", "Move"),
                s("t", "Shuffle"),
                s("c", "Clear history"),
                s("Enter", "Play"),
            ],
            Page::Playlists => vec![
                s("m", "Star selected"),
                s("←/→", "Focus"),
                s("e", "Add"),
                s("i", "Add next"),
                s("t", "Shuffle play"),
                s("Enter", "Play"),
            ],
            Page::Server => vec![
                s("Tab", "Next field"),
                s("Enter", "Test/Save"),
                s("Ctrl+R", "Refresh"),
            ],
            Page::Settings => vec![s("←/→/Enter", "Change")],
        }
    }
}

fn render_binds<'a>(binds: &[(String, String)], colors: &ThemeColors) -> Line<'a> {
    let mut spans = Vec::new();
    for (i, (key, desc)) in binds.iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled(" │ ", Style::default().fg(colors.secondary)));
        }
        spans.push(Span::styled(key.clone(), Style::default().fg(colors.accent)));
        spans.push(Span::raw(":"));
        spans.push(Span::styled(desc.clone(), Style::default().fg(colors.muted)));
    }
    Line::from(spans)
}

impl Widget for Footer<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height < 1 {
            return;
        }

        let chunks = Layout::horizontal([Constraint::Min(40), Constraint::Length(30)]).split(area);
        let left = chunks[0];
        let right = chunks[1];

        if let Some(notif) = self.notification {
            let style = if notif.is_error {
                Style::default().fg(self.colors.error)
            } else {
                Style::default().fg(self.colors.success)
            };
            buf.set_string(left.x, left.y, &notif.message, style);
        } else {
            let global_line = render_binds(&self.global_keybinds(), &self.colors);
            buf.set_line(left.x, left.y, &global_line, left.width);
            if area.height >= 2 {
                let page_line = render_binds(&self.page_keybinds(), &self.colors);
                buf.set_line(left.x, left.y + 1, &page_line, left.width);
            }
        }

        if let Some(rate) = self.sample_rate {
            let khz = rate as f64 / 1000.0;
            let rate_str = if khz == khz.floor() {
                format!("{}kHz", khz as u32)
            } else {
                format!("{:.1}kHz", khz)
            };
            let x = right.x + right.width.saturating_sub(rate_str.len() as u16);
            buf.set_string(
                x,
                right.y,
                &rate_str,
                Style::default().fg(self.colors.success),
            );
        }
    }
}
