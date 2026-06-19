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
        /// Greyed context artist in search (the album matched, not the artist);
        /// still selectable and Enter-expandable into its full catalog.
        greyed: bool,
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
    /// Artist ID from a matched album, enabling Enter-expand of a greyed group.
    id: Option<String>,
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
            id: None,
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
        let group = artist_slot(&mut order, &mut groups, &a.name);
        group.matched = Some(a.clone());
        group.id = Some(a.id.clone());
    }
    // Albums whose own name matches.
    for alb in results
        .album
        .iter()
        .filter(|a| a.name.to_lowercase().contains(&q))
    {
        let artist_name = alb.artist.as_deref().unwrap_or("");
        let group = artist_slot(&mut order, &mut groups, artist_name);
        if group.id.is_none() {
            group.id.clone_from(&alb.artist_id);
        }
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
        if group.id.is_none() {
            group.id.clone_from(&song.artist_id);
        }
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
        // A known artist ID (matched, or parent of a matched album) gives a
        // selectable Enter-expandable row drilling into the full catalog (#28).
        let expand_id = group.id.clone();
        let is_expanded = expand_id.as_deref().is_some_and(|id| expanded.contains(id));
        match (&group.matched, &expand_id) {
            (Some(a), _) => items.push(TreeItem::Artist {
                artist: a.clone(),
                expanded: is_expanded,
                greyed: false,
            }),
            (None, Some(id)) if !group.name.is_empty() => items.push(TreeItem::Artist {
                artist: Artist {
                    id: id.clone(),
                    name: group.name.clone(),
                    album_count: None,
                    cover_art: None,
                },
                expanded: is_expanded,
                greyed: true,
            }),
            // A song with no artist ID renders directly; no blank greyed header.
            (None, None) if !group.name.is_empty() => items.push(TreeItem::ArtistLabel {
                name: group.name.clone(),
            }),
            _ => {}
        }
        if is_expanded {
            if let Some(id) = &expand_id {
                let catalog = albums_cache.get(id).cloned().unwrap_or_default();
                push_expanded_catalog(&mut items, catalog, group);
                continue;
            }
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
        greyed: false,
    });
    if expanded {
        push_sorted_albums(items, albums.map(<[Album]>::to_vec).unwrap_or_default());
    }
}

/// Push album rows sorted by release year (oldest first; undated last).
fn push_sorted_albums(items: &mut Vec<TreeItem>, mut albums: Vec<Album>) {
    sort_albums_by_year(&mut albums);
    items.extend(albums.into_iter().map(|album| TreeItem::Album { album }));
}

/// Sort albums by release year, oldest first, undated last.
fn sort_albums_by_year(albums: &mut [Album]) {
    albums.sort_by(|a, b| match (a.year, b.year) {
        (None, None) => std::cmp::Ordering::Equal,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (Some(_), None) => std::cmp::Ordering::Less,
        (Some(y1), Some(y2)) => y1.cmp(&y2),
    });
}

/// Push an expanded artist's full catalogue, nesting the group's matched songs
/// under the album they belong to so a searched track keeps its place; songs
/// whose album is not in the catalogue, then album-less songs, follow at the end.
fn push_expanded_catalog(items: &mut Vec<TreeItem>, mut catalog: Vec<Album>, group: &SearchArtist) {
    sort_albums_by_year(&mut catalog);
    let mut by_album: HashMap<String, Vec<Child>> = HashMap::new();
    for node in &group.albums {
        if !node.songs.is_empty() {
            by_album.insert(node.name.to_lowercase(), node.songs.clone());
        }
    }
    for album in catalog {
        let key = album.name.to_lowercase();
        items.push(TreeItem::Album { album });
        if let Some(songs) = by_album.remove(&key) {
            items.extend(songs.into_iter().map(|song| TreeItem::Song { song }));
        }
    }
    for node in &group.albums {
        if let Some(songs) = by_album.remove(&node.name.to_lowercase()) {
            items.push(TreeItem::AlbumLabel {
                name: node.name.clone(),
                year: node.year,
            });
            items.extend(songs.into_iter().map(|song| TreeItem::Song { song }));
        }
    }
    for song in &group.direct_songs {
        items.push(TreeItem::Song { song: song.clone() });
    }
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

/// Split `text` into spans, styling case-insensitive runs of `query` with
/// `hit` and the rest with `base`. Bails to one `base` span when the query is
/// empty or lowercasing shifts byte lengths, so it never slices mid-`char`.
fn highlight_spans(text: &str, query: &str, base: Style, hit: Style) -> Vec<Span<'static>> {
    let lower = text.to_lowercase();
    let q = query.to_lowercase();
    if q.is_empty() || lower.len() != text.len() {
        return vec![Span::styled(text.to_string(), base)];
    }
    let mut spans = Vec::new();
    let mut last = 0;
    let mut from = 0;
    while let Some(rel) = lower[from..].find(&q) {
        let start = from + rel;
        let end = start + q.len();
        if !text.is_char_boundary(start) || !text.is_char_boundary(end) {
            break;
        }
        if start > last {
            spans.push(Span::styled(text[last..start].to_string(), base));
        }
        spans.push(Span::styled(text[start..end].to_string(), hit));
        last = end;
        from = end;
    }
    if last < text.len() {
        spans.push(Span::styled(text[last..].to_string(), base));
    }
    if spans.is_empty() {
        spans.push(Span::styled(text.to_string(), base));
    }
    spans
}

/// Render one library-tree row. Selectable rows take their type colour and
/// bold when selected; greyed labels are muted context headers. Occurrences of
/// `query` in the name are accented so the matched text stands out in search.
fn tree_row(
    item: &TreeItem,
    is_selected: bool,
    colors: &ThemeColors,
    query: &str,
) -> ListItem<'static> {
    let styled = |fg| {
        if is_selected {
            Style::default().fg(fg).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(fg)
        }
    };
    let hit = styled(colors.primary).add_modifier(Modifier::BOLD);
    let name_spans = |text: &str, base: Style| highlight_spans(text, query, base, hit);
    match item {
        TreeItem::Artist { artist, greyed, .. } => {
            let base = styled(if *greyed { colors.muted } else { colors.artist });
            ListItem::new(Line::from(name_spans(&artist.name, base)))
        }
        TreeItem::Album { album } => {
            let base = styled(colors.album);
            let mut spans = vec![Span::styled("  └─ ".to_string(), base)];
            spans.extend(name_spans(&album.name, base));
            if let Some(y) = album.year {
                spans.push(Span::styled(format!(" [{y}]"), base));
            }
            ListItem::new(Line::from(spans))
        }
        TreeItem::Song { song } => {
            let base = styled(colors.song);
            let mut spans = vec![Span::styled("      └─ ".to_string(), base)];
            spans.extend(name_spans(&song.title, base));
            ListItem::new(Line::from(spans))
        }
        TreeItem::ArtistLabel { name } => {
            let base = Style::default().fg(colors.muted);
            ListItem::new(Line::from(name_spans(name, base)))
        }
        TreeItem::AlbumLabel { name, year } => {
            let base = Style::default().fg(colors.muted);
            let mut spans = vec![Span::styled("  └─ ".to_string(), base)];
            spans.extend(name_spans(name, base));
            if let Some(y) = year {
                spans.push(Span::styled(format!(" [{y}]"), base));
            }
            ListItem::new(Line::from(spans))
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
            .map(|(i, item)| {
                tree_row(
                    item,
                    Some(i) == artists.selected_index,
                    colors,
                    &artists.filter,
                )
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

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::style::Color;

    fn contents(spans: &[Span<'_>]) -> Vec<String> {
        spans.iter().map(|s| s.content.to_string()).collect()
    }

    #[test]
    fn highlight_splits_the_matched_run_into_its_own_span() {
        let base = Style::default().fg(Color::White);
        let hit = Style::default().fg(Color::Red);
        let spans = highlight_spans("Beach House", "beach", base, hit);
        assert_eq!(contents(&spans), vec!["Beach", " House"]);
        assert_eq!(spans[0].style, hit, "the matched run carries the hit style");
        assert_eq!(spans[1].style, base, "the rest stays base-styled");
    }

    #[test]
    fn highlight_matches_case_insensitively_in_the_middle() {
        let s = Style::default();
        let spans = highlight_spans("The Beach Boys", "BEACH", s, s);
        assert_eq!(contents(&spans), vec!["The ", "Beach", " Boys"]);
    }

    #[test]
    fn highlight_empty_query_is_one_plain_span() {
        let s = Style::default();
        assert_eq!(
            contents(&highlight_spans("Anything", "", s, s)),
            vec!["Anything"]
        );
    }

    #[test]
    fn highlight_no_match_is_one_plain_span() {
        let s = Style::default();
        assert_eq!(
            contents(&highlight_spans("Nirvana", "beach", s, s)),
            vec!["Nirvana"]
        );
    }

    #[test]
    fn highlight_does_not_panic_on_length_changing_lowercase() {
        // 'İ' (U+0130) lowercases to two chars; the guard bails rather than
        // slice mid-char. The full text must survive intact.
        let s = Style::default();
        let joined: String = highlight_spans("İstanbul", "stan", s, s)
            .iter()
            .map(|sp| sp.content.to_string())
            .collect();
        assert_eq!(joined, "İstanbul");
    }
}
