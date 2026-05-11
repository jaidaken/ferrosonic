//! Footer bar with keybind hints and status

use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::Widget,
};

use crate::app::state::{Notification, Page};
use crate::ui::theme::ThemeColors;

/// Footer bar widget
pub struct Footer<'a> {
    page: Page,
    sample_rate: Option<u32>,
    notification: Option<&'a Notification>,
    colors: ThemeColors,
}

impl<'a> Footer<'a> {
    pub fn new(page: Page, colors: ThemeColors) -> Self {
        Self {
            page,
            sample_rate: None,
            notification: None,
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

    fn global_keybinds(&self) -> Vec<(&'static str, &'static str)> {
        vec![
            ("q", "Quit"),
            ("p/Space", "Pause"),
            ("h", "Prev"),
            ("l", "Next"),
            ("n", "Star playing"),
            ("R", "Shuffle library"),
            ("t", "Theme"),
        ]
    }

    fn page_keybinds(&self) -> Vec<(&'static str, &'static str)> {
        match self.page {
            Page::QuickPlay => vec![
                ("m", "Star selected"),
                ("Enter", "Play"),
            ],
            Page::Library => vec![
                ("m", "Star selected"),
                ("/", "Filter"),
                ("←/→", "Focus"),
                ("e", "Add"),
                ("i", "Add next"),
                ("r", "Shuffle"),
                ("Enter", "Play"),
            ],
            Page::Queue => vec![
                ("m", "Star selected"),
                ("d", "Remove"),
                ("J/K", "Move"),
                ("r", "Shuffle"),
                ("c", "Clear history"),
                ("Enter", "Play"),
            ],
            Page::Playlists => vec![
                ("m", "Star selected"),
                ("←/→", "Focus"),
                ("e", "Add"),
                ("i", "Add next"),
                ("r", "Shuffle play"),
                ("Enter", "Play"),
            ],
            Page::Server => vec![
                ("Tab", "Next field"),
                ("Enter", "Test/Save"),
                ("Ctrl+R", "Refresh"),
            ],
            Page::Settings => vec![("←/→/Enter", "Change")],
        }
    }
}

fn render_binds<'a>(
    binds: &[(&'static str, &'static str)],
    colors: &ThemeColors,
) -> Line<'a> {
    let mut spans = Vec::new();
    for (i, (key, desc)) in binds.iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled(" │ ", Style::default().fg(colors.secondary)));
        }
        spans.push(Span::styled(*key, Style::default().fg(colors.accent)));
        spans.push(Span::raw(":"));
        spans.push(Span::styled(*desc, Style::default().fg(colors.muted)));
    }
    Line::from(spans)
}

impl Widget for Footer<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height < 1 {
            return;
        }

        // Split horizontally: left for binds/notification, right for sample rate.
        let chunks = Layout::horizontal([Constraint::Min(40), Constraint::Length(30)]).split(area);
        let left = chunks[0];
        let right = chunks[1];

        // Notification (when present) takes the whole left block and replaces
        // the keybind hints temporarily.
        if let Some(notif) = self.notification {
            let style = if notif.is_error {
                Style::default().fg(self.colors.error)
            } else {
                Style::default().fg(self.colors.success)
            };
            buf.set_string(left.x, left.y, &notif.message, style);
        } else {
            // Row 0: global binds. Row 1: page-specific binds.
            let global_line = render_binds(&self.global_keybinds(), &self.colors);
            buf.set_line(left.x, left.y, &global_line, left.width);
            if area.height >= 2 {
                let page_line = render_binds(&self.page_keybinds(), &self.colors);
                buf.set_line(left.x, left.y + 1, &page_line, left.width);
            }
        }

        // Sample rate, top-right corner.
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
