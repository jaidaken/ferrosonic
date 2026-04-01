//! Main layout and rendering

use ratatui::{
    layout::{Constraint, Layout},
    Frame,
};

use crate::app::state::{AppState, LayoutAreas, Page};

use super::footer::Footer;
use super::header::Header;
use super::pages;
use super::widgets::{CavaWidget, NowPlayingWidget};

/// Draw the entire UI
pub fn draw(frame: &mut Frame, state: &mut AppState) {
    let area = frame.area();

    let cava_active = state.settings_state.cava_enabled && !state.cava_screen.is_empty();

    // Main layout:
    // [Header]          - 1 line
    // [Cava]            - ~40% (optional, only when cava is active)
    // [Page Content]    - flexible
    // [Now Playing]     - 7 lines
    // [Footer]          - 1 line

    let (header_area, cava_area, content_area, now_playing_area, footer_area) = if cava_active {
        let chunks = Layout::vertical([
            Constraint::Length(1),                                         // Header
            Constraint::Percentage(state.settings_state.cava_size as u16), // Cava visualizer
            Constraint::Min(10),                                           // Page content
            Constraint::Length(7),                                         // Now playing
            Constraint::Length(1),                                         // Footer
        ])
        .split(area);
        (chunks[0], Some(chunks[1]), chunks[2], chunks[3], chunks[4])
    } else {
        let chunks = Layout::vertical([
            Constraint::Length(1), // Header
            Constraint::Min(10),   // Page content
            Constraint::Length(7), // Now playing
            Constraint::Length(1), // Footer
        ])
        .split(area);
        (chunks[0], None, chunks[1], chunks[2], chunks[3])
    };

    // Compute dual-pane splits for pages that use them
    let (content_left, content_right) = match state.page {
        Page::Artists | Page::Playlists => {
            let panes =
                Layout::horizontal([Constraint::Percentage(40), Constraint::Percentage(60)])
                    .split(content_area);
            (Some(panes[0]), Some(panes[1]))
        }
        _ => (None, None),
    };

    // Store layout areas for mouse hit-testing
    state.layout = LayoutAreas {
        header: header_area,
        content: content_area,
        now_playing: now_playing_area,
        content_left,
        content_right,
    };

    // Render header
    let colors = *state.settings_state.theme_colors();
    let header = Header::new(state.page, state.now_playing.state, colors);
    frame.render_widget(header, header_area);

    // Render cava visualizer if active
    if let Some(cava_rect) = cava_area {
        let cava_widget = CavaWidget::new(&state.cava_screen);
        frame.render_widget(cava_widget, cava_rect);
    }

    // Render current page
    match state.page {
        Page::Songs => {
            pages::songs::render(frame, content_area, state);
        }
        Page::Artists => {
            pages::artists::render(frame, content_area, state);
        }
        Page::Queue => {
            pages::queue::render(frame, content_area, state);
        }
        Page::Playlists => {
            pages::playlists::render(frame, content_area, state);
        }
        Page::Server => {
            pages::server::render(frame, content_area, state);
        }
        Page::Settings => {
            pages::settings::render(frame, content_area, state);
        }
    }

    // Render now playing
    let now_playing = NowPlayingWidget::new(&state.now_playing, colors);
    frame.render_widget(now_playing, now_playing_area);

    // Render footer
    let footer = Footer::new(state.page, colors)
        .sample_rate(state.now_playing.sample_rate)
        .notification(state.notification.as_ref());
    frame.render_widget(footer, footer_area);
}
