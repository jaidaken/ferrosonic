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
use super::{widget_cava::CavaWidget, widget_now_playing, widget_now_playing::NowPlayingWidget};

const NOW_PLAYING_BASE: u16 = 7;

pub fn draw(
    frame: &mut Frame,
    state: &mut AppState<'_>,
    cover_art_state: &Arc<Mutex<CoverArtState>>,
) {
    let area = frame.area();

    let cava_active =
        state.client.settings_state.cava_enabled && !state.client.cava_screen.is_empty();
    // Dynamic now-playing size: reserve the larger area only when art
    // is actually going to render. Stable across the fetch window
    // because it keys off the song's cover_art id (set on the daemon
    // before bytes arrive), not the protocol-loaded flag.
    let art_visible = state.client.settings_state.cover_art
        && state
            .daemon
            .now_playing
            .song
            .as_ref()
            .and_then(|s| s.cover_art.as_ref())
            .is_some();

    let now_playing_h = if art_visible {
        (state.client.settings_state.cover_art_size as u16).clamp(8, 24)
    } else {
        NOW_PLAYING_BASE
    };

    let band_pct = state.client.settings_state.cava_size as u16;
    let (header_area, cava_area, content_area, now_playing_area, footer_area) = if cava_active {
        let chunks = Layout::vertical([
            Constraint::Length(1),
            Constraint::Percentage(band_pct),
            Constraint::Min(8),
            Constraint::Length(now_playing_h),
            Constraint::Length(2),
        ])
        .split(area);
        (chunks[0], Some(chunks[1]), chunks[2], chunks[3], chunks[4])
    } else {
        let chunks = Layout::vertical([
            Constraint::Length(1),
            Constraint::Min(8),
            Constraint::Length(now_playing_h),
            Constraint::Length(2),
        ])
        .split(area);
        (chunks[0], None, chunks[1], chunks[2], chunks[3])
    };

    let (content_left, content_right) = match state.client.page {
        Page::Library | Page::Playlists => {
            let panes =
                Layout::horizontal([Constraint::Percentage(40), Constraint::Percentage(60)])
                    .split(content_area);
            (Some(panes[0]), Some(panes[1]))
        }
        Page::QuickPlay => {
            let panes = Layout::horizontal([Constraint::Length(22), Constraint::Min(0)])
                .split(content_area);
            (Some(panes[0]), Some(panes[1]))
        }
        _ => (None, None),
    };

    state.client.layout = LayoutAreas {
        header: header_area,
        content: content_area,
        now_playing: now_playing_area,
        content_left,
        content_right,
    };

    let colors = *state.client.settings_state.theme_colors();
    let header = Header::new(state.client.page, state.daemon.now_playing.state, colors);
    frame.render_widget(header, header_area);

    if let Some(cava_rect) = cava_area {
        let cava_widget = CavaWidget::new(&state.client.cava_screen);
        frame.render_widget(cava_widget, cava_rect);
    }

    match state.client.page {
        Page::QuickPlay => pages::songs::render(frame, content_area, state),
        Page::Library => pages::library::render(frame, content_area, state),
        Page::Queue => pages::queue::render(frame, content_area, state),
        Page::Playlists => pages::playlists::render(frame, content_area, state),
        Page::Server => pages::server::render(frame, content_area, state),
        Page::Settings => pages::settings::render(frame, content_area, state),
    }

    // 50/50 horizontal split when art is actually visible. When no
    // art, info uses the full inner width and re-centers naturally.
    let art_cols = if art_visible {
        now_playing_area.width.saturating_sub(2) / 2
    } else {
        0
    };

    let now_playing =
        NowPlayingWidget::new(&state.daemon.now_playing, colors).art_reserved_cols(art_cols);
    frame.render_widget(now_playing, now_playing_area);

    if art_visible {
        let cell_size = cover_art_state
            .try_lock()
            .map(|g| g.cell_size)
            .unwrap_or((10, 20));
        if let Some(rect) = widget_now_playing::art_rect(now_playing_area, art_cols, cell_size) {
            cover_art::render(frame, rect, cover_art_state);
        }
    }

    let footer = Footer::new(state.client.page, colors)
        .sample_rate(state.daemon.now_playing.sample_rate)
        .repeat_mode(state.client.settings_state.repeat_mode)
        .notification(state.client.notification.as_ref());
    frame.render_widget(footer, footer_area);
}
