//! Playlists page with dual-panel browser

use std::sync::{Arc, Mutex};

use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph},
    Frame,
};

use crate::app::state::AppState;
use crate::ui::cover_art::{self, CoverArtState};
use crate::ui::theme::ThemeColors;

/// Render the Playlists page. `playlist_cover_art` holds the highlighted
/// playlist's art, kept separate from the now-playing image.
pub fn render(
    frame: &mut Frame<'_>,
    area: Rect,
    state: &mut AppState<'_>,
    playlist_cover_art: &Arc<Mutex<CoverArtState>>,
) {
    let colors = *state.client.settings_state.theme_colors();

    let (Some(playlist_area), Some(song_area)) =
        crate::ui::layout::content_panes(state.client.page, area)
    else {
        return;
    };
    // Only reserve space once a decoded image is actually present, so the
    // song list never leaves a permanent blank strip while art is absent.
    let art_visible = state.client.settings_state.cover_art
        && playlist_cover_art
            .try_lock()
            .is_ok_and(|guard| guard.image.is_some());
    let (art_area, list_area) = if art_visible {
        split_playlist_art(song_area)
    } else {
        (None, song_area)
    };
    render_playlists(frame, playlist_area, state, &colors);
    render_songs(frame, list_area, state, &colors);
    if let Some(art_rect) = art_area {
        cover_art::render(frame, art_rect, playlist_cover_art);
    }

    if state.client.playlists.renaming {
        let content = format!("{}\u{2588}", state.client.playlists.rename_buf);
        render_edit_box(
            frame,
            playlist_area,
            &content,
            " Rename playlist  (Enter: save  Esc: cancel) ",
            &colors,
        );
    } else if state.client.playlists.confirming_delete {
        let name = state
            .client
            .playlists
            .selected_playlist
            .and_then(|i| state.daemon.library.playlists.get(i))
            .map_or("", |p| p.name.as_str());
        let content = format!("Delete '{name}'?  (y: confirm  n: cancel)");
        render_edit_box(frame, playlist_area, &content, " Delete playlist ", &colors);
    }
}

/// Reserve a top strip of the songs pane for the playlist cover when the pane
/// is large enough; returns the art rect and the remaining song-list rect.
fn split_playlist_art(area: Rect) -> (Option<Rect>, Rect) {
    const MIN_WIDTH: u16 = 30;
    const MIN_HEIGHT: u16 = 12;
    // Always leave at least this many rows for the song list.
    const MIN_SONGS_H: u16 = 6;
    const MAX_ART_H: u16 = 12;
    if area.width < MIN_WIDTH || area.height < MIN_HEIGHT {
        return (None, area);
    }
    let art_h = (area.height / 2)
        .clamp(6, MAX_ART_H)
        .min(area.height.saturating_sub(MIN_SONGS_H));
    if art_h < 6 {
        return (None, area);
    }
    // A square image with roughly 1:2 terminal cell aspect needs ~2x rows of width.
    let art_w = art_h.saturating_mul(2).min(area.width);
    let art = Rect {
        x: area.x,
        y: area.y,
        width: art_w,
        height: art_h,
    };
    let songs = Rect {
        x: area.x,
        y: area.y + art_h,
        width: area.width,
        height: area.height - art_h,
    };
    (Some(art), songs)
}

/// Draw a single-line input/confirmation box over the bottom of `area`.
fn render_edit_box(
    frame: &mut Frame<'_>,
    area: Rect,
    content: &str,
    title: &str,
    colors: &ThemeColors,
) {
    let height = 3;
    let width = area.width.saturating_sub(4).max(10);
    let box_area = Rect {
        x: area.x + 2,
        y: area.y + area.height.saturating_sub(height + 1),
        width,
        height,
    };
    let para = Paragraph::new(content.to_string())
        .style(Style::default().fg(colors.primary))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(title.to_string())
                .border_style(Style::default().fg(colors.primary)),
        );
    frame.render_widget(Clear, box_area);
    frame.render_widget(para, box_area);
}

fn render_playlists(
    frame: &mut Frame<'_>,
    area: Rect,
    state: &mut AppState<'_>,
    colors: &ThemeColors,
) {
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

    let items: Vec<ListItem<'_>> = library_playlists
        .iter()
        .enumerate()
        .map(|(i, playlist)| {
            let is_selected = playlists.selected_playlist == Some(i);

            let count = playlist.song_count.unwrap_or(0);
            let duration = playlist.duration.map(|d| {
                let mins = d / 60;
                let secs = d % 60;
                format!("{mins}:{secs:02}")
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
                    format!(" ({count} songs)"),
                    Style::default().fg(colors.muted),
                ),
            ];

            if let Some(dur) = duration {
                spans.push(Span::styled(
                    format!(" [{dur}]"),
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

fn render_songs(frame: &mut Frame<'_>, area: Rect, state: &mut AppState<'_>, colors: &ThemeColors) {
    let playlists = &state.client.playlists;

    let focused = playlists.focus == 1;
    let border_style = if focused {
        Style::default().fg(colors.border_focused)
    } else {
        Style::default().fg(colors.border_unfocused)
    };

    let title = if playlists.songs.is_empty() {
        " Songs ".to_string()
    } else {
        format!(" Songs ({}) ", playlists.songs.len())
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

    let items: Vec<ListItem<'_>> = playlists
        .songs
        .iter()
        .enumerate()
        .map(|(i, song)| {
            let is_selected = focused && playlists.selected_song == Some(i);
            let is_playing = state.current_song().is_some_and(|s| s.id == song.id);

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
                if artist.is_empty() {
                    Span::raw("")
                } else {
                    Span::styled(format!(" - {artist}"), Style::default().fg(artist_color))
                },
                Span::styled(format!(" [{duration}]"), Style::default().fg(time_color)),
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

#[cfg(test)]
mod tests {
    use super::split_playlist_art;
    use ratatui::layout::Rect;

    #[test]
    fn split_is_skipped_below_the_minimum_size() {
        let (art, songs) = split_playlist_art(Rect::new(0, 0, 29, 40));
        assert!(art.is_none(), "too narrow to show art");
        assert_eq!(songs, Rect::new(0, 0, 29, 40));

        let (art, songs) = split_playlist_art(Rect::new(0, 0, 60, 11));
        assert!(art.is_none(), "too short to show art");
        assert_eq!(songs, Rect::new(0, 0, 60, 11));
    }

    #[test]
    fn split_reserves_a_top_strip_and_keeps_the_song_list_below() {
        let area = Rect::new(3, 5, 60, 20);
        let (art, songs) = split_playlist_art(area);
        let art = art.expect("large enough to show art");
        assert_eq!(art.x, area.x);
        assert_eq!(art.y, area.y);
        assert!(art.height >= 6 && art.height <= 12);
        assert!(art.width <= area.width);
        assert_eq!(songs.x, area.x);
        assert_eq!(songs.y, area.y + art.height);
        assert_eq!(songs.height, area.height - art.height);
        assert_eq!(art.height + songs.height, area.height, "no rows lost");
    }

    #[test]
    fn split_always_leaves_room_for_songs() {
        let area = Rect::new(0, 0, 80, 12);
        let (art, songs) = split_playlist_art(area);
        let art = art.expect("12 rows qualifies");
        assert!(songs.height >= 6, "song list must keep at least 6 rows");
        assert_eq!(art.height + songs.height, area.height);
    }
}
