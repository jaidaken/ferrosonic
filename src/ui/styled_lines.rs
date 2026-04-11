use crate::subsonic::models::Child;
use crate::ui::theme::ThemeColors;

use ratatui::{
    style::{Color, Style},
    text::{Line, Span},
};

pub fn get_song_with_artist_line<'a>(
    song: &Child,
    is_selected: bool,
    is_playing: bool,
    colors: &ThemeColors,
) -> Line<'a> {
    let is_starred = song.starred.is_some();

    let star_indicator = if is_starred { "★ " } else { "  " };
    let indicator = if is_playing { "▶ " } else { "  " };

    let (title_color, artist_color, duration_color) = get_colors(is_selected, is_playing, &colors);
    let artist = match &song.artist {
        Some(value) => value,
        None => "n/a",
    };

    let line = Line::from(vec![
        Span::styled(indicator.to_string(), Style::default().fg(colors.playing)),
        Span::styled(
            star_indicator.to_string(),
            Style::default().fg(colors.playing),
        ),
        Span::styled(song.title.clone(), Style::default().fg(title_color)),
        Span::styled(format!(" - {}", artist), Style::default().fg(artist_color)),
        Span::styled(
            format!(" [{}]", song.format_duration()),
            Style::default().fg(duration_color),
        ),
    ]);

    return line;
}

pub fn get_song_without_artist_line<'a>(
    song: &Child,
    is_selected: bool,
    is_playing: bool,
    has_multiple_discs: bool,
    colors: &ThemeColors,
) -> Line<'a> {
    let is_starred = song.starred.is_some();

    let star_indicator = if is_starred { "★ " } else { "  " };
    let indicator = if is_playing { "▶ " } else { "  " };

    let (title_color, track_color, duration_color) = get_colors(is_selected, is_playing, &colors);

    let track = if has_multiple_discs {
        match (song.disc_number, song.track) {
            (Some(d), Some(t)) => format!("{}.{:02}. ", d, t),
            (None, Some(t)) => format!("{:02}. ", t),
            _ => String::new(),
        }
    } else {
        song.track
            .map(|t| format!("{:02}. ", t))
            .unwrap_or_default()
    };

    let duration = song.format_duration();
    let title = song.title.clone();

    let line = Line::from(vec![
        Span::styled(indicator.to_string(), Style::default().fg(colors.playing)),
        Span::styled(
            star_indicator.to_string(),
            Style::default().fg(colors.playing),
        ),
        Span::styled(track, Style::default().fg(track_color)),
        Span::styled(title, Style::default().fg(title_color)),
        Span::styled(
            format!(" [{}]", duration),
            Style::default().fg(duration_color),
        ),
    ]);
    return line;
}

fn get_colors(is_selected: bool, is_playing: bool, colors: &ThemeColors) -> (Color, Color, Color) {
    return if is_selected {
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
}
