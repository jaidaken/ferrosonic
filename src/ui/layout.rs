//! Main layout and rendering

use std::sync::{Arc, Mutex};

use ratatui::{
    layout::{Constraint, Layout},
    Frame,
};

use crate::app::state::{AppState, LayoutAreas, Page};

use super::cover_art::{self, CoverArtState};
use super::footer::Footer;
use super::header::Header;
use super::pages;
use super::widgets::{CavaWidget, NowPlayingWidget};

/// Draw the entire UI.
///
/// `cover_art` carries the optional decoded image. If cover art is
/// enabled and an image is loaded it shares the cava band with the
/// visualizer; on its own it takes a small dedicated band.
pub fn draw(
    frame: &mut Frame,
    state: &mut AppState<'_>,
    cover_art_state: &Arc<Mutex<CoverArtState>>,
) {
    let area = frame.area();

    let cava_active = state.client.settings_state.cava_enabled && !state.client.cava_screen.is_empty();
    let art_active = state.client.settings_state.cover_art
        && cover_art_state
            .try_lock()
            .map(|g| g.protocol.is_some())
            .unwrap_or(false);

    // Main layout:
    // [Header]          - 1 line
    // [Cava]            - ~40% (optional, only when cava is active)
    // [Page Content]    - flexible
    // [Now Playing]     - 7 lines
    // [Footer]          - 1 line

    // Top band: cava + cover art sit on the same row when both are
    // enabled (split horizontally). Either alone takes the whole band.
    let show_band = cava_active || art_active;
    let band_pct = state.client.settings_state.cava_size as u16;
    let (header_area, band_area, content_area, now_playing_area, footer_area) = if show_band {
        let chunks = Layout::vertical([
            Constraint::Length(1),
            Constraint::Percentage(band_pct),
            Constraint::Min(10),
            Constraint::Length(7),
            Constraint::Length(2),
        ])
        .split(area);
        (chunks[0], Some(chunks[1]), chunks[2], chunks[3], chunks[4])
    } else {
        let chunks = Layout::vertical([
            Constraint::Length(1),
            Constraint::Min(10),
            Constraint::Length(7),
            Constraint::Length(2),
        ])
        .split(area);
        (chunks[0], None, chunks[1], chunks[2], chunks[3])
    };

    let (cava_area, art_area) = match (band_area, cava_active, art_active) {
        (Some(band), true, true) => {
            // Reserve a square-ish region on the left for art, cava
            // takes the rest. Width tied to band height so the art
            // stays roughly proportional to the available row count.
            let art_cols = (band.height as u16).saturating_mul(2).min(band.width / 2);
            let split = Layout::horizontal([
                Constraint::Length(art_cols.max(8)),
                Constraint::Min(0),
            ])
            .split(band);
            (Some(split[1]), Some(split[0]))
        }
        (Some(band), true, false) => (Some(band), None),
        (Some(band), false, true) => (None, Some(band)),
        _ => (None, None),
    };

    // Compute dual-pane splits for pages that use them
    let (content_left, content_right) = match state.client.page {
        Page::Library | Page::Playlists => {
            let panes =
                Layout::horizontal([Constraint::Percentage(40), Constraint::Percentage(60)])
                    .split(content_area);
            (Some(panes[0]), Some(panes[1]))
        }
        Page::QuickPlay => {
            let panes =
                Layout::horizontal([Constraint::Length(22), Constraint::Min(0)])
                    .split(content_area);
            (Some(panes[0]), Some(panes[1]))
        }
        _ => (None, None),
    };

    // Store layout areas for mouse hit-testing
    state.client.layout = LayoutAreas {
        header: header_area,
        content: content_area,
        now_playing: now_playing_area,
        content_left,
        content_right,
    };

    // Render header
    let colors = *state.client.settings_state.theme_colors();
    let header = Header::new(state.client.page, state.daemon.now_playing.state, colors);
    frame.render_widget(header, header_area);

    // Render cava visualizer if active
    if let Some(cava_rect) = cava_area {
        let cava_widget = CavaWidget::new(&state.client.cava_screen);
        frame.render_widget(cava_widget, cava_rect);
    }

    // Render cover art if active and a protocol is loaded.
    if let Some(art_rect) = art_area {
        cover_art::render(frame, art_rect, cover_art_state);
    }

    // Render current page
    match state.client.page {
        Page::QuickPlay => {
            pages::songs::render(frame, content_area, state);
        }
        Page::Library => {
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
    let now_playing = NowPlayingWidget::new(&state.daemon.now_playing, colors);
    frame.render_widget(now_playing, now_playing_area);

    // Render footer
    let footer = Footer::new(state.client.page, colors)
        .sample_rate(state.daemon.now_playing.sample_rate)
        .repeat_mode(state.client.settings_state.repeat_mode)
        .notification(state.client.notification.as_ref());
    frame.render_widget(footer, footer_area);
}
