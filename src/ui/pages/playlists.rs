//! Playlists page with dual-panel browser

use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
    Frame,
};

use crate::app::state::AppState;
use crate::ui::theme::ThemeColors;

pub fn render(frame: &mut Frame, area: Rect, state: &mut AppState<'_>) {
    let colors = *state.client.settings_state.theme_colors();

    let chunks =
        Layout::horizontal([Constraint::Percentage(40), Constraint::Percentage(60)]).split(area);

    render_playlists(frame, chunks[0], state, &colors);
    render_songs(frame, chunks[1], state, &colors);
}

fn render_playlists(frame: &mut Frame, area: Rect, state: &mut AppState<'_>, colors: &ThemeColors) {
    // `playlists` is the per-page UI state (selection, focus, scroll).
    // `library_playlists` is the actual list, owned by the daemon.
    let playlists = &state.client.playlists;
    let library_playlists = &state.daemon.library.playlists;

    let focused = playlists.focus == 0;
    let border_style = if focused {
        Style::default().fg(colors.border_focused)
    } else {
        Style::default().fg(colors.border_unfocused)
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(" Playlists ({}) ", library_playlists.len()))
        .border_style(border_style);

    if library_playlists.is_empty() {
        let hint = Paragraph::new("No playlists found")
            .style(Style::default().fg(colors.muted))
            .block(block);
        frame.render_widget(hint, area);
        return;
    }

    let items: Vec<ListItem> = library_playlists
        .iter()
        .enumerate()
        .map(|(i, playlist)| {
            let is_selected = playlists.selected_playlist == Some(i);

            let count = playlist.song_count.unwrap_or(0);
            let duration = playlist.duration.map(|d| {
                let mins = d / 60;
                let secs = d % 60;
                format!("{}:{:02}", mins, secs)
            });

            let style = if is_selected {
                Style::default()
                    .fg(colors.primary)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(colors.album)
            };

            let mut spans = vec![
                Span::styled(&playlist.name, style),
                Span::styled(
                    format!(" ({} songs)", count),
                    Style::default().fg(colors.muted),
                ),
            ];

            if let Some(dur) = duration {
                spans.push(Span::styled(
                    format!(" [{}]", dur),
                    Style::default().fg(colors.muted),
                ));
            }

            ListItem::new(Line::from(spans))
        })
        .collect();

    let mut list = List::new(items).block(block);
    if focused {
        list = list
            .highlight_style(
                Style::default()
                    .bg(colors.highlight_bg)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("▸ ");
    }

    let mut list_state = ListState::default();
    if focused {
        list_state.select(playlists.selected_playlist);
    }

    frame.render_stateful_widget(list, area, &mut list_state);
    state.client.playlists.playlist_scroll_offset = list_state.offset();
}

fn render_songs(frame: &mut Frame, area: Rect, state: &mut AppState<'_>, colors: &ThemeColors) {
    let playlists = &state.client.playlists;

    let focused = playlists.focus == 1;
    let border_style = if focused {
        Style::default().fg(colors.border_focused)
    } else {
        Style::default().fg(colors.border_unfocused)
    };

    let title = if !playlists.songs.is_empty() {
        format!(" Songs ({}) ", playlists.songs.len())
    } else {
        " Songs ".to_string()
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(border_style);

    if playlists.songs.is_empty() {
        let hint = Paragraph::new("Select a playlist to view songs")
            .style(Style::default().fg(colors.muted))
            .block(block);
        frame.render_widget(hint, area);
        return;
    }

    let items: Vec<ListItem> = playlists
        .songs
        .iter()
        .enumerate()
        .map(|(i, song)| {
            let is_selected = playlists.selected_song == Some(i);
            let is_playing = state
                .current_song()
                .map(|s| s.id == song.id)
                .unwrap_or(false);

            let indicator = if is_playing { "▶ " } else { "  " };
            let artist = song.artist.clone().unwrap_or_default();
            let duration = song.format_duration();

            let (title_color, artist_color, time_color) = if is_selected {
                (
                    colors.highlight_fg,
                    colors.highlight_fg,
                    colors.highlight_fg,
                )
            } else if is_playing {
                (colors.playing, colors.muted, colors.muted)
            } else {
                (colors.song, colors.muted, colors.muted)
            };

            let line = Line::from(vec![
                Span::styled(indicator, Style::default().fg(colors.playing)),
                Span::styled(&song.title, Style::default().fg(title_color)),
                if !artist.is_empty() {
                    Span::styled(format!(" - {}", artist), Style::default().fg(artist_color))
                } else {
                    Span::raw("")
                },
                Span::styled(format!(" [{}]", duration), Style::default().fg(time_color)),
            ]);

            ListItem::new(line)
        })
        .collect();

    let mut list = List::new(items).block(block);
    if focused {
        list = list.highlight_style(
            Style::default()
                .bg(colors.highlight_bg)
                .add_modifier(Modifier::BOLD),
        );
    }

    let mut list_state = ListState::default();
    if focused {
        list_state.select(playlists.selected_song);
    }

    frame.render_stateful_widget(list, area, &mut list_state);
    state.client.playlists.song_scroll_offset = list_state.offset();
}
