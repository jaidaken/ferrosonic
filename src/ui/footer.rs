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
        // `n: Star playing` is shown on the page row next to
        // `m: Star selected` for music pages; on Server / Settings
        // it lives here on the global row so it's still visible.
        let mut v = vec![
            ("q".into(), "Quit".into()),
            ("p/Space".into(), "Pause".into()),
            ("h".into(), "Prev".into()),
            ("l".into(), "Next".into()),
            ("r".into(), repeat_label),
            ("Shift+T".into(), "Shuffle library".into()),
        ];
        if matches!(self.page, Page::Server | Page::Settings) {
            v.push(("n".into(), "Star playing".into()));
        }
        v
    }

    fn page_keybinds(&self) -> Vec<(String, String)> {
        let s = |k: &str, d: &str| (k.to_string(), d.to_string());
        match self.page {
            Page::QuickPlay => vec![
                s("n", "Star playing"),
                s("m", "Star selected"),
                s("Enter", "Play"),
            ],
            Page::Library => vec![
                s("n", "Star playing"),
                s("m", "Star selected"),
                s("/", "Search"),
                s("←/→", "Focus"),
                s("e", "Add"),
                s("i", "Add next"),
                s("t", "Shuffle"),
                s("Enter", "Play"),
            ],
            Page::Queue => vec![
                s("n", "Star playing"),
                s("m", "Star selected"),
                s("d", "Remove"),
                s("J/K", "Move"),
                s("t", "Shuffle"),
                s("c", "Clear history"),
                s("Enter", "Play"),
            ],
            Page::Playlists => vec![
                s("n", "Star playing"),
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
        spans.push(Span::styled(
            key.clone(),
            Style::default().fg(colors.accent),
        ));
        spans.push(Span::raw(":"));
        spans.push(Span::styled(
            desc.clone(),
            Style::default().fg(colors.muted),
        ));
    }
    Line::from(spans)
}

impl Widget for Footer<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height < 1 {
            return;
        }

        // Right column is wider than the sample rate so notifications
        // (which share it on row 1) have room without truncation.
        let chunks = Layout::horizontal([Constraint::Min(40), Constraint::Length(40)]).split(area);
        let left = chunks[0];
        let right = chunks[1];

        // Keybinds always render — notifications no longer hide them.
        let global_line = render_binds(&self.global_keybinds(), &self.colors);
        buf.set_line(left.x, left.y, &global_line, left.width);
        if area.height >= 2 {
            let page_line = render_binds(&self.page_keybinds(), &self.colors);
            buf.set_line(left.x, left.y + 1, &page_line, left.width);
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

        // Notification: row 1, right-aligned under the sample rate.
        // Truncated to the right column's width so it can't bleed
        // back over the page keybinds.
        if area.height >= 2 {
            if let Some(notif) = self.notification {
                let style = if notif.is_error {
                    Style::default().fg(self.colors.error)
                } else {
                    Style::default().fg(self.colors.success)
                };
                let msg: String = notif.message.chars().take(right.width as usize).collect();
                let msg_len = msg.chars().count() as u16;
                let x = right.x + right.width.saturating_sub(msg_len);
                buf.set_string(x, right.y + 1, &msg, style);
            }
        }
    }
}
