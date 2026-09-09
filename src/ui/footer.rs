//! Footer keybind hints + status.

use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::Widget,
};

use crate::app::state::{Notification, Page};
use crate::config::keybind::{default_chord, effective_chord, GlobalAction, KeyChord};
use crate::ui::theme::ThemeColors;

const STATUS_COLUMN_WIDTH: u16 = 40;
const BIND_SEPARATOR_WIDTH: usize = 3;

/// Footer bar widget: shortcuts, sample rate, repeat mode, notifications.
pub struct Footer<'a> {
    page: Page,
    sample_rate: Option<u32>,
    notification: Option<&'a Notification>,
    repeat_mode: crate::config::RepeatMode,
    colors: ThemeColors,
    keybindings: Option<&'a std::collections::HashMap<GlobalAction, KeyChord>>,
}

impl<'a> Footer<'a> {
    /// Footer for `page` with the theme palette.
    #[must_use]
    pub const fn new(page: Page, colors: ThemeColors) -> Self {
        Self {
            page,
            sample_rate: None,
            notification: None,
            repeat_mode: crate::config::RepeatMode::Off,
            colors,
            keybindings: None,
        }
    }

    /// Builder: display the active global keybinding overrides.
    #[must_use]
    pub const fn keybindings(
        mut self,
        bindings: &'a std::collections::HashMap<GlobalAction, KeyChord>,
    ) -> Self {
        self.keybindings = Some(bindings);
        self
    }

    fn chord(&self, action: GlobalAction) -> String {
        self.keybindings
            .map_or_else(
                || default_chord(action),
                |bindings| effective_chord(bindings, action),
            )
            .to_string()
    }

    /// Builder: show the active sample rate.
    #[must_use]
    pub const fn sample_rate(mut self, rate: Option<u32>) -> Self {
        self.sample_rate = rate;
        self
    }

    /// Builder: show a transient notification.
    #[must_use]
    pub const fn notification(mut self, notification: Option<&'a Notification>) -> Self {
        self.notification = notification;
        self
    }

    /// Builder: show the repeat mode indicator.
    #[must_use]
    pub const fn repeat_mode(mut self, mode: crate::config::RepeatMode) -> Self {
        self.repeat_mode = mode;
        self
    }

    fn global_keybinds(&self) -> Vec<(String, String)> {
        let repeat_label = format!("Repeat ({})", self.repeat_mode.label());
        // `n: Star playing` is shown on the page row next to
        // `m: Star selected` for music pages; on Server / Settings
        // it lives here on the global row so it's still visible.
        let pause = self.chord(GlobalAction::TogglePause);
        let pause_hint = if pause == "p" {
            "p".to_string()
        } else {
            format!("p/{pause}")
        };
        let mut v = vec![
            (self.chord(GlobalAction::Quit), "Quit".into()),
            (pause_hint, "Pause".into()),
            (self.chord(GlobalAction::PreviousTrack), "Prev".into()),
            (self.chord(GlobalAction::NextTrack), "Next".into()),
            (self.chord(GlobalAction::CycleRepeat), repeat_label),
            (self.chord(GlobalAction::ToggleLyrics), "Lyrics".into()),
            (
                self.chord(GlobalAction::ShuffleLibrary),
                "Shuffle library".into(),
            ),
        ];
        if matches!(self.page, Page::Server | Page::Settings) {
            v.push((self.chord(GlobalAction::StarPlaying), "Star playing".into()));
        }
        v
    }

    fn page_keybinds(&self) -> Vec<(String, String)> {
        let s = |k: &str, d: &str| (k.to_string(), d.to_string());
        match self.page {
            Page::QuickPlay => vec![
                (self.chord(GlobalAction::StarPlaying), "Star playing".into()),
                s("m", "Star selected"),
                s("Enter", "Play"),
            ],
            Page::Library => vec![
                (self.chord(GlobalAction::StarPlaying), "Star playing".into()),
                s("m", "Star selected"),
                s("/", "Search"),
                s("←/→", "Focus"),
                s("v", "Albums/Artists"),
                s("f", "Library"),
                s("s", "Sort"),
                s("e", "Add"),
                s("i", "Add next"),
                s("t", "Shuffle"),
                s("Enter", "Play"),
            ],
            Page::Queue => vec![
                (self.chord(GlobalAction::StarPlaying), "Star playing".into()),
                s("m", "Star selected"),
                s("d", "Remove"),
                s("J/K", "Move"),
                s("t", "Shuffle"),
                s("s", "Save playlist"),
                s("c", "Clear history"),
                s("Enter", "Play"),
            ],
            Page::Playlists => vec![
                s("Enter", "Play"),
                s("e/i", "Queue"),
                s("a", "→ Playlist"),
                s("R", "Rename"),
                s("D", "Delete"),
                s("d", "Remove"),
                s("J/K", "Reorder"),
                s("t", "Shuffle"),
                s("m", "Star"),
            ],
            Page::Server => vec![
                s("Tab", "Next field"),
                s("Enter", "Test/Save"),
                (self.chord(GlobalAction::Refresh), "Refresh".into()),
            ],
            Page::Settings => vec![s("←/→/Enter", "Change")],
        }
    }

    fn uses_status_column(&self, width: u16) -> bool {
        let widest_binds = render_binds(&self.global_keybinds(), &self.colors)
            .width()
            .max(render_binds(&self.page_keybinds(), &self.colors).width());
        usize::from(width) >= widest_binds.saturating_add(usize::from(STATUS_COLUMN_WIDTH))
    }

    /// Rows needed to display every shortcut without splitting a key/description
    /// pair. Narrow, tall terminals trade spare vertical space for full hints.
    #[must_use]
    pub fn required_height(&self, width: u16) -> u16 {
        if width == 0 {
            return 0;
        }
        if self.uses_status_column(width) {
            return 2;
        }

        let bind_rows = wrapped_binds(&self.global_keybinds(), &self.colors, width)
            .len()
            .saturating_add(wrapped_binds(&self.page_keybinds(), &self.colors, width).len());
        let status_rows = usize::from(self.sample_rate.is_some() || self.notification.is_some());
        crate::num::u16_sat(bind_rows.saturating_add(status_rows))
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

fn bind_spans(key: &str, desc: &str, colors: &ThemeColors) -> Vec<Span<'static>> {
    vec![
        Span::styled(key.to_string(), Style::default().fg(colors.accent)),
        Span::raw(":"),
        Span::styled(desc.to_string(), Style::default().fg(colors.muted)),
    ]
}

fn wrapped_binds<'a>(
    binds: &[(String, String)],
    colors: &ThemeColors,
    width: u16,
) -> Vec<Line<'a>> {
    if width == 0 {
        return Vec::new();
    }

    let width = usize::from(width);
    let mut lines = Vec::new();
    let mut spans = Vec::new();
    let mut line_width = 0usize;

    for (key, desc) in binds {
        let bind_width = Line::from(format!("{key}:{desc}")).width();
        if !spans.is_empty()
            && line_width
                .saturating_add(BIND_SEPARATOR_WIDTH)
                .saturating_add(bind_width)
                > width
        {
            lines.push(Line::from(std::mem::take(&mut spans)));
            line_width = 0;
        }
        if !spans.is_empty() {
            spans.push(Span::styled(" │ ", Style::default().fg(colors.secondary)));
            line_width = line_width.saturating_add(BIND_SEPARATOR_WIDTH);
        }
        spans.extend(bind_spans(key, desc, colors));
        line_width = line_width.saturating_add(bind_width);
    }
    if !spans.is_empty() {
        lines.push(Line::from(spans));
    }
    lines
}

fn sample_rate_text(rate: u32) -> String {
    let khz = f64::from(rate) / 1000.0;
    // Exact integer-value check; floor() is exact, an epsilon compare would be wrong.
    #[allow(clippy::float_cmp)]
    if khz == khz.floor() {
        // f64->u32 `as` saturates; khz is a positive sample-rate/1000.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let khz_int = khz as u32;
        format!("{khz_int}kHz")
    } else {
        format!("{khz:.1}kHz")
    }
}

impl Widget for Footer<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height < 1 {
            return;
        }

        let side_by_side = self.uses_status_column(area.width);
        let (bind_width, right) = if side_by_side {
            let chunks =
                Layout::horizontal([Constraint::Min(0), Constraint::Length(STATUS_COLUMN_WIDTH)])
                    .split(area);
            (chunks[0].width, Some(chunks[1]))
        } else {
            (area.width, None)
        };

        let mut bind_lines = wrapped_binds(&self.global_keybinds(), &self.colors, bind_width);
        bind_lines.extend(wrapped_binds(
            &self.page_keybinds(),
            &self.colors,
            bind_width,
        ));
        for (row, line) in bind_lines.iter().enumerate() {
            let row = crate::num::u16_sat(row);
            if row >= area.height {
                break;
            }
            buf.set_line(area.x, area.y + row, line, bind_width);
        }

        if let (Some(rate), Some(right)) = (self.sample_rate, right) {
            let rate_str = sample_rate_text(rate);
            let x = right.x
                + right
                    .width
                    .saturating_sub(crate::num::u16_sat(rate_str.len()));
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
        if side_by_side && area.height >= 2 {
            if let Some(notif) = self.notification {
                let right = right.unwrap_or(area);
                let style = if notif.is_error {
                    Style::default().fg(self.colors.error)
                } else {
                    Style::default().fg(self.colors.success)
                };
                let msg: String = notif.message.chars().take(right.width as usize).collect();
                let msg_len = crate::num::u16_sat(msg.chars().count());
                let x = right.x + right.width.saturating_sub(msg_len);
                buf.set_string(x, right.y + 1, &msg, style);
            }
        } else if !side_by_side && bind_lines.len() < usize::from(area.height) {
            let status_y = area.y + crate::num::u16_sat(bind_lines.len());
            let rate_str = self.sample_rate.map(sample_rate_text);
            let rate_width = rate_str.as_ref().map_or(0, |s| s.chars().count());
            if let Some(notif) = self.notification {
                let style = if notif.is_error {
                    Style::default().fg(self.colors.error)
                } else {
                    Style::default().fg(self.colors.success)
                };
                let gap = usize::from(rate_str.is_some());
                let available = usize::from(area.width)
                    .saturating_sub(rate_width)
                    .saturating_sub(gap);
                let msg: String = notif.message.chars().take(available).collect();
                buf.set_string(area.x, status_y, msg, style);
            }
            if let Some(rate_str) = rate_str {
                let x = area.x + area.width.saturating_sub(crate::num::u16_sat(rate_width));
                buf.set_string(
                    x,
                    status_y,
                    rate_str,
                    Style::default().fg(self.colors.success),
                );
            }
        }
    }
}
