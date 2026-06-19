//! Queue page showing current play queue

use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph},
    Frame,
};

use crate::app::state::AppState;

/// Render the Queue page.
pub fn render(frame: &mut Frame<'_>, area: Rect, state: &mut AppState<'_>) {
    let colors = *state.client.settings_state.theme_colors();

    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(" Queue ({}) ", state.daemon.queue.len()))
        .border_style(Style::default().fg(colors.border_focused));

    if state.daemon.queue.is_empty() {
        let hint = Paragraph::new("Queue is empty. Add songs from Artists or Playlists.")
            .style(Style::default().fg(colors.muted))
            .block(block);
        frame.render_widget(hint, area);
        return;
    }

    let items: Vec<ListItem<'_>> = state
        .daemon
        .queue
        .iter()
        .enumerate()
        .map(|(i, song)| {
            let is_current = state.daemon.queue_position == Some(i);
            let is_selected = state.client.queue_state.selected == Some(i);
            let is_played = state
                .daemon
                .queue_position
                .map(|pos| i < pos)
                .unwrap_or(false);
            let is_starred = song.starred.is_some();

            let indicator = if is_current { "▶ " } else { "  " };
            let star_indicator = if is_starred { "★ " } else { "  " };

            let artist = song.artist.clone().unwrap_or_default();
            let duration = song.format_duration();
            let track_info = match (song.disc_number, song.track) {
                (Some(d), Some(t)) if d > 1 => format!(" [{}.{}]", d, t),
                (_, Some(t)) => format!(" [#{}]", t),
                _ => String::new(),
            };

            let (title_style, artist_style, number_style) = if is_current {
                (
                    Style::default()
                        .fg(colors.playing)
                        .add_modifier(Modifier::BOLD),
                    Style::default().fg(colors.playing),
                    Style::default().fg(colors.playing),
                )
            } else if is_played {
                (
                    if is_selected {
                        Style::default()
                            .fg(colors.played)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(colors.played)
                    },
                    Style::default().fg(colors.muted),
                    Style::default().fg(colors.muted),
                )
            } else if is_selected {
                (
                    Style::default()
                        .fg(colors.primary)
                        .add_modifier(Modifier::BOLD),
                    Style::default().fg(colors.muted),
                    Style::default().fg(colors.muted),
                )
            } else {
                (
                    Style::default().fg(colors.song),
                    Style::default().fg(colors.muted),
                    Style::default().fg(colors.muted),
                )
            };

            let line = Line::from(vec![
                Span::styled(format!("{:3}. ", i + 1), number_style),
                Span::styled(indicator, Style::default().fg(colors.playing)),
                Span::styled(
                    star_indicator.to_string(),
                    Style::default().fg(colors.playing),
                ),
                Span::styled(song.title.clone(), title_style),
                Span::styled(track_info, Style::default().fg(colors.muted)),
                if !artist.is_empty() {
                    Span::styled(format!(" - {}", artist), artist_style)
                } else {
                    Span::raw("")
                },
                Span::styled(
                    format!(" [{}]", duration),
                    Style::default().fg(colors.muted),
                ),
            ]);

            ListItem::new(line)
        })
        .collect();

    let list = List::new(items)
        .block(block)
        .highlight_style(Style::default().bg(colors.highlight_bg))
        .highlight_symbol("▸ ");

    let mut list_state = ListState::default();
    list_state.select(state.client.queue_state.selected);

    frame.render_stateful_widget(list, area, &mut list_state);
    state.client.queue_state.scroll_offset = list_state.offset();

    if state.client.queue_state.naming_playlist {
        render_name_box(
            frame,
            area,
            &state.client.queue_state.playlist_name,
            &colors,
        );
    }
}

/// Draw the save-as-playlist input box over the bottom of the queue area.
fn render_name_box(
    frame: &mut Frame<'_>,
    area: Rect,
    name: &str,
    colors: &crate::ui::theme::ThemeColors,
) {
    let height = 3;
    let width = area.width.saturating_sub(4).max(10);
    let box_area = Rect {
        x: area.x + 2,
        y: area.y + area.height.saturating_sub(height + 1),
        width,
        height,
    };
    let input = Paragraph::new(format!("{name}\u{2588}"))
        .style(Style::default().fg(colors.primary))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Save as playlist  (Enter: save  Esc: cancel) ")
                .border_style(Style::default().fg(colors.primary)),
        );
    frame.render_widget(Clear, box_area);
    frame.render_widget(input, box_area);
}
