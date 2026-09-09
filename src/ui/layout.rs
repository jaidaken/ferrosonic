//! Top-level frame layout and draw entry point.

use std::sync::{Arc, Mutex};

use ratatui::{
    layout::{Constraint, Layout, Rect},
    Frame,
};

use crate::app::state::{AppState, LayoutAreas, Page};

use super::cover_art::{self, CoverArtState};
use super::footer::Footer;
use super::header::Header;
use super::pages;
use super::{widget_cava::CavaWidget, widget_now_playing, widget_now_playing::NowPlayingWidget};

const NOW_PLAYING_BASE: u16 = 7;
const STACKED_PANE_WIDTH: u16 = 72;
const STACKED_QUICK_PLAY_WIDTH: u16 = 64;

/// Split a two-pane page according to the available content dimensions.
#[must_use]
pub fn content_panes(page: Page, area: Rect) -> (Option<Rect>, Option<Rect>) {
    let panes = match page {
        Page::Library | Page::Playlists if area.width < STACKED_PANE_WIDTH => {
            Layout::vertical([Constraint::Percentage(40), Constraint::Percentage(60)]).split(area)
        }
        Page::Library | Page::Playlists => {
            Layout::horizontal([Constraint::Percentage(40), Constraint::Percentage(60)]).split(area)
        }
        Page::QuickPlay if area.width < STACKED_QUICK_PLAY_WIDTH => {
            let options_height = if area.height <= 8 { 4 } else { 6 };
            Layout::vertical([Constraint::Length(options_height), Constraint::Min(0)]).split(area)
        }
        Page::QuickPlay => {
            Layout::horizontal([Constraint::Length(22), Constraint::Min(0)]).split(area)
        }
        _ => return (None, None),
    };
    (Some(panes[0]), Some(panes[1]))
}

/// Draw one full frame: header, page content, now-playing, footer.
// Cohesive single match/render; splitting would fragment one logical unit.
#[allow(clippy::too_many_lines)]
pub fn draw(
    frame: &mut Frame<'_>,
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

    let desired_now_playing_h = if art_visible {
        u16::from(state.client.settings_state.cover_art_size).clamp(8, 24)
    } else {
        NOW_PLAYING_BASE
    };

    let band_pct = u16::from(state.client.settings_state.cava_size);
    // Form pages draw at fixed rows; a larger floor makes the cava band
    // yield space instead of starving them into a blank page.
    let content_min = match state.client.page {
        Page::Settings | Page::Server => 20,
        _ => 8,
    };
    let colors = *state.client.settings_state.theme_colors();
    let footer_h = Footer::new(state.client.page, colors)
        .sample_rate(state.daemon.now_playing.sample_rate)
        .repeat_mode(state.client.settings_state.repeat_mode)
        .notification(state.client.notification.as_ref())
        .keybindings(&state.client.settings_state.keybindings)
        .required_height(area.width);
    let header_h = Header::required_height(area.width);
    let fixed_ui_h = header_h.saturating_add(footer_h);
    let remaining_h = area.height.saturating_sub(fixed_ui_h);
    let content_floor = content_min.min(remaining_h);
    let now_playing_h = desired_now_playing_h.min(remaining_h.saturating_sub(content_floor));
    let spare_h = remaining_h.saturating_sub(content_floor.saturating_add(now_playing_h));
    let cava_h = if cava_active {
        (area.height.saturating_mul(band_pct) / 100).min(spare_h)
    } else {
        0
    };
    let (header_area, cava_area, content_area, now_playing_area, footer_area) = if cava_active {
        let chunks = Layout::vertical([
            Constraint::Length(header_h),
            Constraint::Length(cava_h),
            Constraint::Min(content_floor),
            Constraint::Length(now_playing_h),
            Constraint::Length(footer_h),
        ])
        .split(area);
        (chunks[0], Some(chunks[1]), chunks[2], chunks[3], chunks[4])
    } else {
        let chunks = Layout::vertical([
            Constraint::Length(header_h),
            Constraint::Min(content_floor),
            Constraint::Length(now_playing_h),
            Constraint::Length(footer_h),
        ])
        .split(area);
        (chunks[0], None, chunks[1], chunks[2], chunks[3])
    };

    let (content_left, content_right) = content_panes(state.client.page, content_area);

    state.client.layout = LayoutAreas {
        header: header_area,
        content: content_area,
        now_playing: now_playing_area,
        content_left,
        content_right,
    };

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
    let art_cols = if art_visible && now_playing_area.width >= 64 && now_playing_area.height >= 8 {
        (now_playing_area.width.saturating_sub(2) / 2).min(32)
    } else {
        0
    };

    let now_playing =
        NowPlayingWidget::new(&state.daemon.now_playing, colors).art_reserved_cols(art_cols);
    frame.render_widget(now_playing, now_playing_area);

    if art_visible {
        let cell_size = cover_art_state.try_lock().map_or((10, 20), |g| g.cell_size);
        if let Some(rect) = widget_now_playing::art_rect(now_playing_area, art_cols, cell_size) {
            cover_art::render(frame, rect, cover_art_state);
        }
    }

    let footer = Footer::new(state.client.page, colors)
        .sample_rate(state.daemon.now_playing.sample_rate)
        .repeat_mode(state.client.settings_state.repeat_mode)
        .notification(state.client.notification.as_ref())
        .keybindings(&state.client.settings_state.keybindings);
    frame.render_widget(footer, footer_area);

    if state.client.quit_prompt {
        super::quit_prompt::render(frame, area, &colors);
    }

    if state.client.playlist_picker.active {
        super::playlist_picker::render(frame, area, state, &colors);
    }

    if state.client.settings_state.filter_editor.is_some()
        || state.client.settings_state.keybinding_editor.is_some()
    {
        super::settings_editor::render(frame, area, state, &colors);
    }

    if state.client.lyrics.open {
        super::lyrics::render(frame, area, state, &colors);
    }
}
