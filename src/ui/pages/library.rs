//! Library (artists) page.

use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
    Frame,
};

use crate::app::state::{AppState, FilterScope};
use crate::subsonic::models::{Album, Artist, Child};
use crate::ui::styled_lines::get_song_without_artist_line;
use crate::ui::theme::ThemeColors;

#[derive(Clone)]
/// One row of the library tree.
pub enum TreeItem {
    /// Artist row.
    Artist {
        /// The artist shown.
        artist: Artist,
        /// Whether its albums are expanded.
        expanded: bool,
    },
    /// Album row under an expanded artist.
    Album {
        /// The album shown.
        album: Album,
    },
    /// Song row in search results.
    Song {
        /// The song shown.
        song: Child,
    },
}

/// Search-results path takes over when the filter is non-empty and a
/// reply has landed; otherwise walks the library tree.
pub fn build_tree_items(state: &AppState<'_>) -> Vec<TreeItem> {
    let ui = &state.client.artists;
    let albums_cache = &state.daemon.library.albums_cache;

    if !ui.filter.is_empty() {
        if let Some(results) = &ui.search_results {
            return match ui.filter_scope {
                FilterScope::Artists => {
                    let mut items = Vec::new();
                    for a in &results.artist {
                        push_artist_with_albums(
                            &mut items,
                            a,
                            ui.expanded.contains(&a.id),
                            albums_cache.get(&a.id).map(Vec::as_slice),
                        );
                    }
                    items
                }
                FilterScope::Albums => results
                    .album
                    .iter()
                    .map(|a| TreeItem::Album { album: a.clone() })
                    .collect(),
                FilterScope::Songs => results
                    .song
                    .iter()
                    .map(|s| TreeItem::Song { song: s.clone() })
                    .collect(),
            };
        }
    }

    let library_artists = &state.daemon.library.artists;
    let filtered_artists: Vec<_> = if ui.filter.is_empty() {
        library_artists.iter().collect()
    } else {
        let filter_lower = ui.filter.to_lowercase();
        library_artists
            .iter()
            .filter(|a| a.name.to_lowercase().contains(&filter_lower))
            .collect()
    };

    let mut items = Vec::new();
    for artist in filtered_artists {
        push_artist_with_albums(
            &mut items,
            artist,
            ui.expanded.contains(&artist.id),
            albums_cache.get(&artist.id).map(Vec::as_slice),
        );
    }
    items
}

/// Push an artist row, then its albums (release-year sorted) when expanded.
/// Shared by the search and tree paths so both drill into albums identically.
fn push_artist_with_albums(
    items: &mut Vec<TreeItem>,
    artist: &Artist,
    expanded: bool,
    albums: Option<&[Album]>,
) {
    items.push(TreeItem::Artist {
        artist: artist.clone(),
        expanded,
    });
    let Some(albums) = albums.filter(|_| expanded) else {
        return;
    };
    let mut sorted: Vec<Album> = albums.to_vec();
    sorted.sort_by(|a, b| match (a.year, b.year) {
        (None, None) => std::cmp::Ordering::Equal,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (Some(_), None) => std::cmp::Ordering::Less,
        (Some(y1), Some(y2)) => y1.cmp(&y2),
    });
    items.extend(sorted.into_iter().map(|album| TreeItem::Album { album }));
}

/// Render the Library page.
pub fn render(frame: &mut Frame<'_>, area: Rect, state: &mut AppState<'_>) {
    let colors = *state.client.settings_state.theme_colors();

    let chunks =
        Layout::horizontal([Constraint::Percentage(40), Constraint::Percentage(60)]).split(area);

    render_tree(frame, chunks[0], state, &colors);
    render_songs(frame, chunks[1], state, &colors);
}

fn render_tree(frame: &mut Frame<'_>, area: Rect, state: &mut AppState<'_>, colors: &ThemeColors) {
    let artists = &state.client.artists;

    let focused = artists.focus == 0;
    let searching = artists.filter_active || !artists.filter.is_empty();

    // Searching takes priority over the focus colour so an active
    // search is always visually obvious.
    let border_style = if searching {
        Style::default()
            .fg(colors.accent)
            .add_modifier(Modifier::BOLD)
    } else if focused {
        Style::default().fg(colors.border_focused)
    } else {
        Style::default().fg(colors.border_unfocused)
    };

    let scope_label = artists.filter_scope.label();
    // Slash count mirrors the cycle depth — `/` artists, `//` albums,
    // `///` songs — so the prompt itself tells you which scope you're
    // searching across.
    let scope_slashes = match artists.filter_scope {
        FilterScope::Artists => "/",
        FilterScope::Albums => "//",
        FilterScope::Songs => "///",
    };
    let album_view = artists.view == crate::app::page_state::LibraryView::AlbumList;

    let base_block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style);
    let block = if searching {
        base_block.title(format!(
            " Search {} ({}{}) ",
            scope_label, scope_slashes, artists.filter
        ))
    } else {
        // Toggle hint: Artists <-> Albums. The active mode shows in its accent
        // colour, the other label and the arrow are muted; they flip on 'v'.
        let muted = Style::default().fg(colors.muted);
        let artists_style = if album_view {
            muted
        } else {
            Style::default().fg(colors.artist)
        };
        let albums_style = if album_view {
            Style::default().fg(colors.album)
        } else {
            muted
        };
        let mut spans = vec![
            Span::styled(" Artists ", artists_style),
            Span::styled("\u{2192} ", muted),
            Span::styled("Albums", albums_style),
        ];
        if album_view {
            let label = artists.album_sort.label();
            spans.push(Span::styled(format!(" \u{00b7} {label} "), muted));
        } else {
            spans.push(Span::raw(" "));
        }
        base_block.title(Line::from(spans))
    };

    let items: Vec<ListItem<'_>> = if album_view {
        artists
            .albums
            .iter()
            .enumerate()
            .map(|(i, album)| {
                let album_style = if Some(i) == artists.album_selected {
                    Style::default()
                        .fg(colors.album)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(colors.album)
                };
                let muted = Style::default().fg(colors.muted);
                let mut spans = vec![Span::styled(album.name.clone(), album_style)];
                if let Some(y) = album.sort_year() {
                    spans.push(Span::styled(format!(" [{y}]"), muted));
                }
                let artist = album.artist.as_deref().unwrap_or("");
                if !artist.is_empty() {
                    spans.push(Span::styled(format!("  {artist}"), muted));
                }
                ListItem::new(Line::from(spans))
            })
            .collect()
    } else {
        build_tree_items(state)
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

                        let year_str = album.year.map(|y| format!(" [{}]", y)).unwrap_or_default();
                        let text = if !artists.filter.is_empty()
                            && artists.search_results.is_some()
                            && artists.filter_scope == FilterScope::Albums
                        {
                            let artist = album.artist.as_deref().unwrap_or("");
                            if artist.is_empty() {
                                format!("{}{}", album.name, year_str)
                            } else {
                                format!("{} — {}{}", artist, album.name, year_str)
                            }
                        } else {
                            format!("  └─ {}{}", album.name, year_str)
                        };

                        ListItem::new(text).style(style)
                    }
                    TreeItem::Song { song } => {
                        let style = if is_selected {
                            Style::default()
                                .fg(colors.song)
                                .add_modifier(Modifier::BOLD)
                        } else {
                            Style::default().fg(colors.song)
                        };
                        let artist = song.artist.as_deref().unwrap_or("");
                        let text = if artist.is_empty() {
                            song.title.clone()
                        } else {
                            format!("{} — {}", artist, song.title)
                        };
                        ListItem::new(text).style(style)
                    }
                }
            })
            .collect()
    };

    let mut list = List::new(items).block(block);
    if focused {
        list = list.highlight_style(
            Style::default()
                .bg(colors.highlight_bg)
                .add_modifier(Modifier::BOLD),
        );
    }

    let mut list_state = ListState::default();
    *list_state.offset_mut() = if album_view {
        state.client.artists.album_scroll_offset
    } else {
        state.client.artists.tree_scroll_offset
    };
    if focused {
        list_state.select(if album_view {
            state.client.artists.album_selected
        } else {
            state.client.artists.selected_index
        });
    }

    frame.render_stateful_widget(list, area, &mut list_state);
    if album_view {
        state.client.artists.album_scroll_offset = list_state.offset();
    } else {
        state.client.artists.tree_scroll_offset = list_state.offset();
    }
}

fn render_songs(frame: &mut Frame<'_>, area: Rect, state: &mut AppState<'_>, colors: &ThemeColors) {
    let artists = &state.client.artists;

    let focused = artists.focus == 1;
    let border_style = if focused {
        Style::default().fg(colors.border_focused)
    } else {
        Style::default().fg(colors.border_unfocused)
    };

    let title = if artists.songs.is_empty() {
        " Songs ".to_string()
    } else {
        let first = artists.songs.first();
        let album = first.and_then(|s| s.album.as_deref());
        let artist = first
            .and_then(|s| s.artist.as_deref())
            .filter(|a| !a.is_empty());
        match (artist, album) {
            (Some(ar), Some(al)) => format!(" {ar} \u{2014} {al} "),
            (None, Some(al)) => format!(" {al} "),
            _ => " Songs ".to_string(),
        }
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

    let has_multiple_discs = artists
        .songs
        .iter()
        .any(|s| s.disc_number.map(|d| d > 1).unwrap_or(false));

    let items: Vec<ListItem<'_>> = artists
        .songs
        .iter()
        .enumerate()
        .map(|(i, song)| {
            let is_selected = focused && Some(i) == artists.selected_song;
            let is_playing = state
                .current_song()
                .map(|s| s.id == song.id)
                .unwrap_or(false);

            let line = get_song_without_artist_line(
                song,
                is_selected,
                is_playing,
                has_multiple_discs,
                colors,
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
    *list_state.offset_mut() = state.client.artists.song_scroll_offset;
    if focused {
        list_state.select(artists.selected_song);
    }

    frame.render_stateful_widget(list, area, &mut list_state);
    state.client.artists.song_scroll_offset = list_state.offset();
}
