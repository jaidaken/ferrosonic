//! Header bar: page tabs and transport buttons.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Widget,
};

use crate::app::state::Page;
use crate::daemon::state::PlaybackState;
use crate::ui::theme::ThemeColors;

/// Header bar widget: page tabs plus transport buttons.
pub struct Header {
    current_page: Page,
    playback_state: PlaybackState,
    colors: ThemeColors,
}

impl Header {
    /// Header for the current page and playback state.
    #[must_use]
    pub const fn new(
        current_page: Page,
        playback_state: PlaybackState,
        colors: ThemeColors,
    ) -> Self {
        Self {
            current_page,
            playback_state,
            colors,
        }
    }

    /// Rows needed to show every page tab and the transport controls.
    #[must_use]
    pub fn required_height(width: u16) -> u16 {
        header_regions(Rect::new(0, 0, width, u16::MAX))
            .iter()
            .map(|(_, rect)| rect.y.saturating_add(rect.height))
            .max()
            .unwrap_or(1)
            .max(1)
    }
}

impl Widget for Header {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height < 1 {
            return;
        }

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

        // U+FE0E forces narrow text rendering; without it some terminals render
        // these media symbols emoji-wide and the buttons misalign.
        let controls = Line::from(vec![
            Span::styled(" \u{23EE}\u{FE0E} ", nav_style),
            Span::raw(" "),
            Span::styled(" \u{23F5}\u{FE0E} ", play_style),
            Span::raw(" "),
            Span::styled(" \u{23F8}\u{FE0E} ", pause_style),
            Span::raw(" "),
            Span::styled(" \u{23F9}\u{FE0E} ", stop_style),
            Span::raw(" "),
            Span::styled(" \u{23ED}\u{FE0E} ", nav_style),
        ]);

        for (region, rect) in header_regions(area) {
            if rect.y >= area.y.saturating_add(area.height) {
                continue;
            }
            match region {
                HeaderRegion::Tab(page) => {
                    let style = if page == self.current_page {
                        Style::default()
                            .fg(self.colors.primary)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default()
                    };
                    buf.set_string(
                        rect.x,
                        rect.y,
                        format!(" {} {} ", page.shortcut(), page.label()),
                        style,
                    );
                }
                HeaderRegion::PrevButton => {
                    buf.set_line(rect.x, rect.y, &controls, 19);
                }
                HeaderRegion::PlayButton
                | HeaderRegion::PauseButton
                | HeaderRegion::StopButton
                | HeaderRegion::NextButton => {}
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Clickable region of the header, resolved from mouse coordinates.
pub enum HeaderRegion {
    /// A page tab.
    Tab(Page),
    /// Previous-track button.
    PrevButton,
    /// Play button.
    PlayButton,
    /// Pause button.
    PauseButton,
    /// Stop button.
    StopButton,
    /// Next-track button.
    NextButton,
}

impl Header {
    /// Map a click position to its header region, if any.
    #[must_use]
    pub fn region_at(area: Rect, x: u16, y: u16) -> Option<HeaderRegion> {
        header_regions(area).into_iter().find_map(|(region, rect)| {
            (x >= rect.x
                && x < rect.x.saturating_add(rect.width)
                && y >= rect.y
                && y < rect.y.saturating_add(rect.height))
            .then_some(region)
        })
    }
}

const PAGES: [Page; 6] = [
    Page::Library,
    Page::Queue,
    Page::QuickPlay,
    Page::Playlists,
    Page::Server,
    Page::Settings,
];
const CONTROLS_WIDTH: u16 = 19;

/// Compute both render and hit-test rectangles so wrapped headers stay clickable.
fn header_regions(area: Rect) -> Vec<(HeaderRegion, Rect)> {
    let mut regions = Vec::with_capacity(PAGES.len() + 5);
    let mut row = 0u16;
    let mut cursor = 0u16;

    for page in PAGES {
        let width = crate::num::u16_sat(page.shortcut().len() + page.label().len() + 3);
        if cursor > 0 && cursor.saturating_add(width) > area.width {
            row = row.saturating_add(1);
            cursor = 0;
        }
        regions.push((
            HeaderRegion::Tab(page),
            Rect::new(area.x + cursor, area.y + row, width.min(area.width), 1),
        ));
        cursor = cursor.saturating_add(width).saturating_add(1);
    }

    if cursor > 0 && cursor.saturating_add(CONTROLS_WIDTH) > area.width {
        row = row.saturating_add(1);
    }
    let start = area.x + area.width.saturating_sub(CONTROLS_WIDTH);
    for (region, offset) in [
        (HeaderRegion::PrevButton, 0),
        (HeaderRegion::PlayButton, 4),
        (HeaderRegion::PauseButton, 8),
        (HeaderRegion::StopButton, 12),
        (HeaderRegion::NextButton, 16),
    ] {
        regions.push((region, Rect::new(start + offset, area.y + row, 3, 1)));
    }
    regions
}
