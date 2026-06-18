//! Library (artists) page.

use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
    Frame,
};

use std::collections::HashMap;

use crate::app::state::AppState;
use crate::subsonic::models::{Album, Artist, Child, SearchResult3};
use crate::ui::styled_lines::get_song_without_artist_line;
use crate::ui::theme::ThemeColors;
use std::collections::HashSet;

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
    /// Greyed, non-selectable artist header grouping matched albums in search.
    ArtistLabel {
        /// The parent artist's name.
        name: String,
    },
    /// Greyed, non-selectable album header grouping matched songs in search.
    AlbumLabel {
        /// The parent album's name.
        name: String,
        /// The album's release year, if known.
        year: Option<i32>,
    },
}

/// One album node within a search artist group: a selectable matched album
/// (`album`) or a greyed context header, with any matched songs beneath it.
struct SearchAlbum {
    album: Option<Album>,
    name: String,
    year: Option<i32>,
    songs: Vec<Child>,
}

/// One artist group within the search tree, keyed by lowercased artist name.
struct SearchArtist {
    matched: Option<Artist>,
    name: String,
    albums: Vec<SearchAlbum>,
    album_idx: HashMap<String, usize>,
    direct_songs: Vec<Child>,
}

/// Get or create the artist group for `name`, preserving first-seen order.
fn artist_slot<'m>(
    order: &mut Vec<String>,
    groups: &'m mut HashMap<String, SearchArtist>,
    name: &str,
) -> &'m mut SearchArtist {
    let key = name.to_lowercase();
    groups.entry(key.clone()).or_insert_with(|| {
        order.push(key);
        SearchArtist {
            matched: None,
            name: name.to_string(),
            albums: Vec::new(),
            album_idx: HashMap::new(),
            direct_songs: Vec::new(),
        }
    })
}

/// Get or create the album node for `name` within an artist group.
fn album_slot<'a>(
    artist: &'a mut SearchArtist,
    name: &str,
    year: Option<i32>,
) -> &'a mut SearchAlbum {
    let key = name.to_lowercase();
    if let Some(&i) = artist.album_idx.get(&key) {
        return &mut artist.albums[i];
    }
    let i = artist.albums.len();
    artist.album_idx.insert(key, i);
    artist.albums.push(SearchAlbum {
        album: None,
        name: name.to_string(),
        year,
        songs: Vec::new(),
    });
    &mut artist.albums[i]
}

/// Build the search tree: a match-depth view where every row matches the
/// query on its own name. Artist matches render as a row (Enter expands the
/// full cached catalog); album-name matches nest under their artist (greyed
/// when the artist did not match), no songs until selected; title matches nest
/// under album under artist. `search3` also matches via artist/album, so album
/// and song results are re-filtered to own-name hits.
fn build_search_items(
    filter: &str,
    expanded: &HashSet<String>,
    albums_cache: &HashMap<String, Vec<Album>>,
    results: &SearchResult3,
) -> Vec<TreeItem> {
    let q = filter.to_lowercase();
    let mut order: Vec<String> = Vec::new();
    let mut groups: HashMap<String, SearchArtist> = HashMap::new();

    // Matched artists lead the order.
    for a in &results.artist {
        artist_slot(&mut order, &mut groups, &a.name).matched = Some(a.clone());
    }
    // Albums whose own name matches.
    for alb in results
        .album
        .iter()
        .filter(|a| a.name.to_lowercase().contains(&q))
    {
        let artist_name = alb.artist.as_deref().unwrap_or("");
        let group = artist_slot(&mut order, &mut groups, artist_name);
        let node = album_slot(group, &alb.name, alb.year);
        node.album = Some(alb.clone());
        if node.year.is_none() {
            node.year = alb.year;
        }
    }
    // Songs whose own title matches.
    for song in results
        .song
        .iter()
        .filter(|s| s.title.to_lowercase().contains(&q))
    {
        let artist_name = song.artist.as_deref().unwrap_or("");
        let group = artist_slot(&mut order, &mut groups, artist_name);
        match song.album.as_deref() {
            Some(album_name) if !album_name.is_empty() => {
                album_slot(group, album_name, song.year)
                    .songs
                    .push(song.clone());
            }
            _ => group.direct_songs.push(song.clone()),
        }
    }

    let mut items = Vec::new();
    for key in &order {
        let group = &groups[key];
        match &group.matched {
            Some(a) => {
                let is_expanded = expanded.contains(&a.id);
                items.push(TreeItem::Artist {
                    artist: a.clone(),
                    expanded: is_expanded,
                });
                // Expanding a matched artist drills into its full cached catalog
                // (issue #28: reach albums no name matched); collapsed stops here.
                if is_expanded {
                    push_sorted_albums(
                        &mut items,
                        albums_cache.get(&a.id).cloned().unwrap_or_default(),
                    );
                    continue;
                }
            }
            // A song with no artist renders directly; no blank greyed header.
            None if !group.name.is_empty() => items.push(TreeItem::ArtistLabel {
                name: group.name.clone(),
            }),
            None => {}
        }
        for node in &group.albums {
            match &node.album {
                Some(album) => items.push(TreeItem::Album {
                    album: album.clone(),
                }),
                None => items.push(TreeItem::AlbumLabel {
                    name: node.name.clone(),
                    year: node.year,
                }),
            }
            for song in &node.songs {
                items.push(TreeItem::Song { song: song.clone() });
            }
        }
        for song in &group.direct_songs {
            items.push(TreeItem::Song { song: song.clone() });
        }
    }
    items
}

/// Search-results path takes over when the filter is non-empty and a
/// reply has landed; otherwise walks the library tree.
pub fn build_tree_items(state: &AppState<'_>) -> Vec<TreeItem> {
    let ui = &state.client.artists;
    let albums_cache = &state.daemon.library.albums_cache;

    if !ui.filter.is_empty() {
        if let Some(results) = &ui.search_results {
            return build_search_items(&ui.filter, &ui.expanded, albums_cache, results);
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
    if expanded {
        push_sorted_albums(items, albums.map(<[Album]>::to_vec).unwrap_or_default());
    }
}

/// Push album rows sorted by release year (oldest first; undated last).
fn push_sorted_albums(items: &mut Vec<TreeItem>, mut albums: Vec<Album>) {
    albums.sort_by(|a, b| match (a.year, b.year) {
        (None, None) => std::cmp::Ordering::Equal,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (Some(_), None) => std::cmp::Ordering::Less,
        (Some(y1), Some(y2)) => y1.cmp(&y2),
    });
    items.extend(albums.into_iter().map(|album| TreeItem::Album { album }));
}

/// Render one flat album-list row: name, optional year, then muted artist.
fn album_row(album: &Album, is_selected: bool, colors: &ThemeColors) -> ListItem<'static> {
    let album_style = if is_selected {
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
}

/// Render one library-tree row. Selectable rows take their type colour and
/// bold when selected; greyed labels are muted context headers.
fn tree_row(item: &TreeItem, is_selected: bool, colors: &ThemeColors) -> ListItem<'static> {
    let styled = |fg| {
        if is_selected {
            Style::default().fg(fg).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(fg)
        }
    };
    match item {
        TreeItem::Artist { artist, .. } => {
            ListItem::new(artist.name.clone()).style(styled(colors.artist))
        }
        TreeItem::Album { album } => {
            let year_str = album.year.map(|y| format!(" [{y}]")).unwrap_or_default();
            ListItem::new(format!("  └─ {}{year_str}", album.name)).style(styled(colors.album))
        }
        TreeItem::Song { song } => {
            ListItem::new(format!("      └─ {}", song.title)).style(styled(colors.song))
        }
        TreeItem::ArtistLabel { name } => {
            ListItem::new(name.clone()).style(Style::default().fg(colors.muted))
        }
        TreeItem::AlbumLabel { name, year } => {
            let year_str = year.map(|y| format!(" [{y}]")).unwrap_or_default();
            ListItem::new(format!("  └─ {name}{year_str}")).style(Style::default().fg(colors.muted))
        }
    }
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

    let album_view = artists.view == crate::app::page_state::LibraryView::AlbumList;

    let base_block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style);
    let block = if searching {
        base_block.title(format!(" Search ({}) ", artists.filter))
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
            .map(|(i, album)| album_row(album, Some(i) == artists.album_selected, colors))
            .collect()
    } else {
        build_tree_items(state)
            .iter()
            .enumerate()
            .map(|(i, item)| tree_row(item, Some(i) == artists.selected_index, colors))
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
