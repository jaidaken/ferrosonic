//! Add-to-playlist picker modal: choose a target playlist for a song.

use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::Line,
    widgets::{Block, Borders, Clear, List, ListItem, ListState},
    Frame,
};

use crate::app::state::AppState;
use crate::ui::theme::ThemeColors;

/// Draw a centered playlist picker listing every playlist over the frame.
pub fn render(frame: &mut Frame<'_>, area: Rect, state: &AppState<'_>, colors: &ThemeColors) {
    let playlists = &state.daemon.library.playlists;
    let picker = &state.client.playlist_picker;

    let w = 60.min(area.width);
    let rows = (playlists.len() as u16).clamp(1, 14);
    let h = (rows + 2).min(area.height);
    if w < 4 || h < 4 {
        return;
    }
    let x = area.x + area.width.saturating_sub(w) / 2;
    let y = area.y + area.height.saturating_sub(h) / 2;
    let rect = Rect::new(x, y, w, h);

    let song = picker.song.as_ref().map_or("song", |s| s.title.as_str());
    let title = format!(" Add '{song}' to…  (Enter: add  Esc: cancel) ");

    let items: Vec<ListItem<'_>> = playlists
        .iter()
        .map(|p| {
            let count = p.song_count.unwrap_or(0);
            ListItem::new(Line::from(format!("{}  ({count} songs)", p.name)))
        })
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(colors.accent))
                .title(title),
        )
        .highlight_style(
            Style::default()
                .bg(colors.highlight_bg)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▸ ");

    let mut list_state = ListState::default();
    if !playlists.is_empty() {
        list_state.select(Some(picker.selected.min(playlists.len() - 1)));
    }

    frame.render_widget(Clear, rect);
    frame.render_stateful_widget(list, rect, &mut list_state);
}
