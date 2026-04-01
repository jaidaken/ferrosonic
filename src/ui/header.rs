//! Header bar with page tabs and playback controls

use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Tabs, Widget},
};

use crate::app::state::{Page, PlaybackState};
use crate::ui::theme::ThemeColors;

/// Header bar widget
pub struct Header {
    current_page: Page,
    playback_state: PlaybackState,
    colors: ThemeColors,
}

impl Header {
    pub fn new(current_page: Page, playback_state: PlaybackState, colors: ThemeColors) -> Self {
        Self {
            current_page,
            playback_state,
            colors,
        }
    }
}

impl Widget for Header {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height < 1 {
            return;
        }

        // Split header: [tabs] [playback controls]
        let chunks = Layout::horizontal([Constraint::Min(40), Constraint::Length(30)]).split(area);

        // Page tabs
        let titles: Vec<Line> = [
            Page::Songs,
            Page::Artists,
            Page::Queue,
            Page::Playlists,
            Page::Server,
            Page::Settings,
        ]
        .iter()
        .map(|p: &Page| Line::from(format!("{} {}", p.shortcut(), p.label())))
        .collect();

        let tabs = Tabs::new(titles)
            .select(self.current_page.index())
            .highlight_style(
                Style::default()
                    .fg(self.colors.primary)
                    .add_modifier(Modifier::BOLD),
            )
            .divider(" │ ");

        tabs.render(chunks[0], buf);

        // Playback controls
        let nav_style = Style::default().fg(self.colors.muted);
        let play_style = match self.playback_state {
            PlaybackState::Playing => Style::default().fg(self.colors.accent),
            _ => Style::default().fg(self.colors.muted),
        };
        let pause_style = match self.playback_state {
            PlaybackState::Paused => Style::default().fg(self.colors.accent),
            _ => Style::default().fg(self.colors.muted),
        };
        let stop_style = match self.playback_state {
            PlaybackState::Stopped => Style::default().fg(self.colors.accent),
            _ => Style::default().fg(self.colors.muted),
        };

        let controls = Line::from(vec![
            Span::styled(" ⏮ ", nav_style),
            Span::raw(" "),
            Span::styled(" ▶ ", play_style),
            Span::raw(" "),
            Span::styled(" ⏸ ", pause_style),
            Span::raw(" "),
            Span::styled(" ⏹ ", stop_style),
            Span::raw(" "),
            Span::styled(" ⏭ ", nav_style),
        ]);

        // Right-align controls - " ⏮  ▶  ⏸  ⏹  ⏭ " = 5*3 + 4*1 = 19
        let controls_width = 19;
        let x = chunks[1].x + chunks[1].width.saturating_sub(controls_width);
        buf.set_line(x, chunks[1].y, &controls, controls_width);
    }
}

/// Clickable region in the header
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeaderRegion {
    Tab(Page),
    PrevButton,
    PlayButton,
    PauseButton,
    StopButton,
    NextButton,
}

impl Header {
    /// Determine which region was clicked
    pub fn region_at(area: Rect, x: u16, _y: u16) -> Option<HeaderRegion> {
        let chunks = Layout::horizontal([Constraint::Min(40), Constraint::Length(30)]).split(area);

        if x >= chunks[0].x && x < chunks[0].x + chunks[0].width {
            // Tab area — compute actual tab positions matching Tabs widget rendering.
            // Tabs renders: [pad][title][pad] [divider] [pad][title][pad] ...
            // Default padding = 1 space each side. Divider = " │ " = 3 chars.
            let pages = [
                Page::Songs,
                Page::Artists,
                Page::Queue,
                Page::Playlists,
                Page::Server,
                Page::Settings,
            ];
            let divider_width: u16 = 3; // " │ "
            let padding: u16 = 1; // 1 space each side

            let rel_x = x - chunks[0].x;
            let mut cursor: u16 = 0;
            for (i, page) in pages.iter().enumerate() {
                let label = format!("{} {}", page.shortcut(), page.label());
                let tab_width = padding + label.len() as u16 + padding;
                if rel_x >= cursor && rel_x < cursor + tab_width {
                    return Some(HeaderRegion::Tab(*page));
                }
                cursor += tab_width;
                // Add divider (except after the last tab)
                if i < pages.len() - 1 {
                    cursor += divider_width;
                }
            }
            return None;
        }

        if x >= chunks[1].x && x < chunks[1].x + chunks[1].width {
            // Controls area — rendered as spans:
            // " ⏮ " + " " + " ▶ " + " " + " ⏸ " + " " + " ⏹ " + " " + " ⏭ "
            // Each button span: space + icon + space = 3 display cells (icon is 1 cell)
            // Gap spans: 1 space each
            // Total: 5*3 + 4*1 = 19 display cells
            let controls_width: u16 = 19;
            let control_start = chunks[1].x + chunks[1].width.saturating_sub(controls_width);
            if x >= control_start {
                let offset = x - control_start;
                // Layout: [0..2] ⏮  [3] gap  [4..6] ▶  [7] gap  [8..10] ⏸  [11] gap  [12..14] ⏹  [15] gap  [16..18] ⏭
                return match offset {
                    0..=2 => Some(HeaderRegion::PrevButton),
                    4..=6 => Some(HeaderRegion::PlayButton),
                    8..=10 => Some(HeaderRegion::PauseButton),
                    12..=14 => Some(HeaderRegion::StopButton),
                    16..=18 => Some(HeaderRegion::NextButton),
                    _ => None,
                };
            }
        }

        None
    }
}
