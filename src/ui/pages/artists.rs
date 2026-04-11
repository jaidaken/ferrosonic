//! Artists page with tree browser and song list

use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
    Frame,
};

use crate::app::state::AppState;
use crate::subsonic::models::{Album, Artist};
use crate::ui::styled_lines::get_song_without_artist_line;
use crate::ui::theme::ThemeColors;

/// A tree item - either an artist or an album
#[derive(Clone)]
pub enum TreeItem {
    Artist { artist: Artist, expanded: bool },
    Album { album: Album },
}

/// Build flattened tree items from state
pub fn build_tree_items(state: &AppState) -> Vec<TreeItem> {
    let artists = &state.artists;
    let mut items = Vec::new();

    // Filter artists by name
    let filtered_artists: Vec<_> = if artists.filter.is_empty() {
        artists.artists.iter().collect()
    } else {
        let filter_lower = artists.filter.to_lowercase();
        artists
            .artists
            .iter()
            .filter(|a| a.name.to_lowercase().contains(&filter_lower))
            .collect()
    };

    for artist in filtered_artists {
        let is_expanded = artists.expanded.contains(&artist.id);
        items.push(TreeItem::Artist {
            artist: artist.clone(),
            expanded: is_expanded,
        });

        // If expanded, add albums sorted by year (oldest first)
        if is_expanded {
            if let Some(albums) = artists.albums_cache.get(&artist.id) {
                let mut sorted_albums: Vec<Album> = albums.to_vec();
                sorted_albums.sort_by(|a, b| {
                    // Albums with no year go last
                    match (a.year, b.year) {
                        (None, None) => std::cmp::Ordering::Equal,
                        (None, Some(_)) => std::cmp::Ordering::Greater,
                        (Some(_), None) => std::cmp::Ordering::Less,
                        (Some(y1), Some(y2)) => std::cmp::Ord::cmp(&y1, &y2),
                    }
                });
                for album in sorted_albums {
                    items.push(TreeItem::Album { album });
                }
            }
        }
    }

    items
}

/// Render the artists page
pub fn render(frame: &mut Frame, area: Rect, state: &mut AppState) {
    let colors = *state.settings_state.theme_colors();

    // Split into two panes: [Tree Browser] [Song List]
    let chunks =
        Layout::horizontal([Constraint::Percentage(40), Constraint::Percentage(60)]).split(area);

    render_tree(frame, chunks[0], state, &colors);
    render_songs(frame, chunks[1], state, &colors);
}

/// Render the artist/album tree
fn render_tree(frame: &mut Frame, area: Rect, state: &mut AppState, colors: &ThemeColors) {
    let artists = &state.artists;

    let focused = artists.focus == 0;
    let border_style = if focused {
        Style::default().fg(colors.border_focused)
    } else {
        Style::default().fg(colors.border_unfocused)
    };

    let title = if artists.filter_active {
        format!(" Artists (/{}) ", artists.filter)
    } else if !artists.filter.is_empty() {
        format!(" Artists [{}] ", artists.filter)
    } else {
        " Artists ".to_string()
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(border_style);

    let tree_items = build_tree_items(state);

    // Build list items from tree
    let items: Vec<ListItem> = tree_items
        .iter()
        .enumerate()
        .map(|(i, item)| {
            let is_selected = Some(i) == artists.selected_index;

            match item {
                TreeItem::Artist {
                    artist,
                    expanded: _,
                } => {
                    let style = if is_selected {
                        Style::default()
                            .fg(colors.artist)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(colors.artist)
                    };

                    ListItem::new(artist.name.clone()).style(style)
                }
                TreeItem::Album { album } => {
                    let style = if is_selected {
                        Style::default()
                            .fg(colors.album)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(colors.album)
                    };

                    // Indent albums with tree-style connector, show year in brackets
                    let year_str = album.year.map(|y| format!(" [{}]", y)).unwrap_or_default();
                    let text = format!("  └─ {}{}", album.name, year_str);

                    ListItem::new(text).style(style)
                }
            }
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
    *list_state.offset_mut() = state.artists.tree_scroll_offset;
    if focused {
        list_state.select(state.artists.selected_index);
    }

    frame.render_stateful_widget(list, area, &mut list_state);
    state.artists.tree_scroll_offset = list_state.offset();
}

/// Render the song list for selected album
fn render_songs(frame: &mut Frame, area: Rect, state: &mut AppState, colors: &ThemeColors) {
    let artists = &state.artists;

    let focused = artists.focus == 1;
    let border_style = if focused {
        Style::default().fg(colors.border_focused)
    } else {
        Style::default().fg(colors.border_unfocused)
    };

    let title = if !artists.songs.is_empty() {
        if let Some(album) = artists.songs.first().and_then(|s| s.album.as_ref()) {
            format!(" {} ({}) ", album, artists.songs.len())
        } else {
            format!(" Songs ({}) ", artists.songs.len())
        }
    } else {
        " Songs ".to_string()
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(border_style);

    if artists.songs.is_empty() {
        let hint = Paragraph::new("Select an album to view songs")
            .style(Style::default().fg(colors.muted))
            .block(block);
        frame.render_widget(hint, area);
        return;
    }

    // Check if album has multiple discs
    let has_multiple_discs = artists
        .songs
        .iter()
        .any(|s| s.disc_number.map(|d| d > 1).unwrap_or(false));

    // Build song list items
    let items: Vec<ListItem> = artists
        .songs
        .iter()
        .enumerate()
        .map(|(i, song)| {
            let is_selected = Some(i) == artists.selected_song;
            let is_playing = state
                .current_song()
                .map(|s| s.id == song.id)
                .unwrap_or(false);

            let line = get_song_without_artist_line(
                &song,
                is_selected,
                is_playing,
                has_multiple_discs,
                &colors,
            );
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
    *list_state.offset_mut() = state.artists.song_scroll_offset;
    if focused {
        list_state.select(artists.selected_song);
    }

    frame.render_stateful_widget(list, area, &mut list_state);
    state.artists.song_scroll_offset = list_state.offset();
}
