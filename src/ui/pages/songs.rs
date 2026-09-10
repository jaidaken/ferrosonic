//! Quick Play page renderer.

use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
    Frame,
};

use crate::app::models::SongOption;
use crate::app::state::AppState;
use crate::ui::styled_lines::get_song_with_artist_line;
use crate::ui::theme::ThemeColors;
use strum::IntoEnumIterator;

/// Render the Quick Play page.
pub fn render(frame: &mut Frame<'_>, area: Rect, state: &mut AppState<'_>) {
    let colors = *state.client.settings_state.theme_colors();

    let (Some(option_area), Some(song_area)) =
        crate::ui::layout::content_panes(state.client.page, area)
    else {
        return;
    };
    render_options(frame, option_area, state, &colors);
    render_songs(frame, song_area, state, &colors);
}

fn render_options(frame: &mut Frame<'_>, area: Rect, state: &AppState<'_>, colors: &ThemeColors) {
    let focus = state.client.songs.focus;
    let selected_option = state
        .client
        .songs
        .selected_option
        .unwrap_or(SongOption::Starred);

    let focused = focus == 0;
    let border_style = if focused {
        Style::default().fg(colors.border_focused)
    } else {
        Style::default().fg(colors.border_unfocused)
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .title("Song Options")
        .border_style(border_style);

    let columns = option_columns(area);
    if columns > 1 {
        let column_width = (usize::from(area.width.saturating_sub(2)) / columns).max(1);
        let options = SongOption::iter().collect::<Vec<_>>();
        let lines = options
            .chunks(columns)
            .map(|row| {
                Line::from(
                    row.iter()
                        .map(|option| {
                            let selected = *option == selected_option;
                            let mut style = Style::default().fg(if selected {
                                colors.highlight_fg
                            } else {
                                colors.song
                            });
                            if selected {
                                style = style.add_modifier(Modifier::BOLD);
                                if focused {
                                    style = style.bg(colors.highlight_bg);
                                }
                            }
                            Span::styled(option_cell(*option, column_width), style)
                        })
                        .collect::<Vec<_>>(),
                )
            })
            .collect::<Vec<_>>();
        frame.render_widget(Paragraph::new(lines).block(block), area);
        return;
    }

    let items = SongOption::iter().map(|option| {
        let is_selected = option == selected_option;

        let title_color = if is_selected {
            colors.highlight_fg
        } else {
            colors.song
        };

        ListItem::new(Span::styled(
            option.to_string(),
            Style::default().fg(title_color),
        ))
    });

    let mut list = List::new(items).block(block);
    let mut highlight_style = Style::default().add_modifier(Modifier::BOLD);

    if focused {
        highlight_style = highlight_style.bg(colors.highlight_bg);
    }

    list = list.highlight_style(highlight_style);

    let mut list_state = ListState::default();
    list_state.select(Some(selected_option as usize));

    frame.render_stateful_widget(list, area, &mut list_state);
}

/// Number of option columns needed to keep every Quick Play mode visible.
#[must_use]
pub(crate) fn option_columns(area: Rect) -> usize {
    let rows = usize::from(area.height.saturating_sub(2)).max(1);
    let count = SongOption::iter().count();
    count.div_ceil(rows).max(1)
}

fn option_cell(option: SongOption, width: usize) -> String {
    let label_width = width.saturating_sub(1);
    let label = option
        .compact_label()
        .chars()
        .take(label_width)
        .collect::<String>();
    format!("{label:<width$}")
}

fn render_songs(frame: &mut Frame<'_>, area: Rect, state: &mut AppState<'_>, colors: &ThemeColors) {
    let selected_option = state
        .client
        .songs
        .selected_option
        .unwrap_or(SongOption::Starred);
    // Resolve the daemon-owned list backing the selected Quick Play mode.
    let library_songs: Vec<crate::subsonic::models::Child> = state.songs_list().to_vec();

    let focused = state.client.songs.focus == 1;
    let border_style = if focused {
        Style::default().fg(colors.border_focused)
    } else {
        Style::default().fg(colors.border_unfocused)
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .title("Songs")
        .border_style(border_style);

    if library_songs.is_empty() {
        state.client.songs.scroll_offset = 0;
        frame.render_widget(
            Paragraph::new(format!("No songs available for {selected_option}.")).block(block),
            area,
        );
        return;
    }

    let songs_ui = &state.client.songs;

    let items: Vec<ListItem<'_>> = library_songs
        .iter()
        .enumerate()
        .map(|(i, song)| {
            let is_selected = Some(i) == songs_ui.selected_index && focused;

            let is_playing = state.current_song().is_some_and(|s| s.id == song.id);

            ListItem::new(get_song_with_artist_line(
                song,
                is_selected,
                is_playing,
                colors,
            ))
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
    *list_state.offset_mut() = state.client.songs.scroll_offset;
    if focused {
        list_state.select(state.client.songs.selected_index);
    }

    frame.render_stateful_widget(list, area, &mut list_state);
    state.client.songs.scroll_offset = list_state.offset();
}
