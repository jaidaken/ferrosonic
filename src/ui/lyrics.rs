//! Global lyrics overlay.

use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    Frame,
};

use crate::app::page_state::LyricsStatus;
use crate::app::state::AppState;
use crate::subsonic::models::LyricsSource;
use crate::ui::theme::ThemeColors;

fn source_language(source: &LyricsSource) -> &str {
    match source.lang.as_deref() {
        None | Some("" | "xxx" | "und") => "unspecified",
        Some(language) => language,
    }
}

#[allow(clippy::cast_precision_loss)]
fn active_line(source: &LyricsSource, position_seconds: f64) -> Option<usize> {
    if !source.synced {
        return None;
    }
    let position_ms = position_seconds.max(0.0) * 1000.0;
    source
        .lines
        .iter()
        .enumerate()
        .filter_map(|(index, line)| {
            let adjusted = line.start? as f64 + source.offset as f64;
            (adjusted <= position_ms).then_some(index)
        })
        .next_back()
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)]
fn estimated_line(
    source: &LyricsSource,
    position_seconds: f64,
    duration_seconds: f64,
) -> Option<usize> {
    if source.synced || source.lines.is_empty() || duration_seconds <= 0.0 {
        return None;
    }
    let progress = (position_seconds / duration_seconds).clamp(0.0, 1.0);
    Some(
        ((progress * source.lines.len() as f64) as usize).min(source.lines.len().saturating_sub(1)),
    )
}

fn wrapped_line_height(value: &str, width: usize) -> usize {
    let width = width.max(1);
    Line::from(format!("  {value}"))
        .width()
        .max(1)
        .div_ceil(width)
}

/// Draw the lyrics overlay over the completed application frame.
// Loading/error/ready layouts share the same modal geometry and are clearer together.
#[allow(clippy::too_many_lines)]
pub fn render(frame: &mut Frame<'_>, area: Rect, state: &mut AppState<'_>, colors: &ThemeColors) {
    let width = area.width.saturating_sub(2).min(88);
    let height = area.height.saturating_sub(2).min(32);
    if width < 18 || height < 7 {
        return;
    }
    let rect = Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    );
    let song_title = state
        .daemon
        .now_playing
        .song
        .as_ref()
        .map_or("Lyrics", |song| song.title.as_str());
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(colors.accent))
        .title(format!(" Lyrics — {song_title} "));
    let inner = block.inner(rect);

    frame.render_widget(Clear, rect);
    frame.render_widget(block, rect);

    match &state.client.lyrics.status {
        LyricsStatus::Idle | LyricsStatus::Loading => {
            frame.render_widget(Paragraph::new("Loading lyrics…").centered(), inner);
        }
        LyricsStatus::Empty => {
            frame.render_widget(
                Paragraph::new("No lyrics found for this track.\n\nr: Retry   y/Esc: Close")
                    .centered(),
                inner,
            );
        }
        LyricsStatus::Error(error) => {
            frame.render_widget(
                Paragraph::new(format!(
                    "Could not load lyrics\n\n{error}\n\nr: Retry   y/Esc: Close"
                ))
                .style(Style::default().fg(colors.error))
                .centered()
                .wrap(Wrap { trim: false }),
                inner,
            );
        }
        LyricsStatus::Ready(sources) => {
            let selected = state
                .client
                .lyrics
                .selected_source
                .min(sources.len().saturating_sub(1));
            let Some(source) = sources.get(selected) else {
                return;
            };
            let help_height = if inner.width < 60 { 2 } else { 1 };
            let chunks = Layout::vertical([
                Constraint::Length(1),
                Constraint::Min(1),
                Constraint::Length(help_height),
            ])
            .split(inner);
            let timing = if source.synced {
                "synced"
            } else if inner.width < 60 {
                "unsynced · follow≈"
            } else {
                "unsynced · estimated follow"
            };
            let source_hint = if inner.width >= 60 {
                " · ←/→ change"
            } else {
                ""
            };
            frame.render_widget(
                Paragraph::new(format!(
                    " Source {}/{} · {} · {}{}",
                    selected + 1,
                    sources.len(),
                    source_language(source),
                    timing,
                    source_hint,
                ))
                .style(Style::default().fg(colors.muted)),
                chunks[0],
            );

            let active = active_line(source, state.daemon.now_playing.position);
            let follow_target = active.or_else(|| {
                estimated_line(
                    source,
                    state.daemon.now_playing.position,
                    state.daemon.now_playing.duration,
                )
            });
            let body_width = usize::from(chunks[1].width);
            if state.client.lyrics.follow {
                if let Some(follow_target) = follow_target {
                    let target_row = source
                        .lines
                        .iter()
                        .take(follow_target)
                        .map(|line| wrapped_line_height(&line.value, body_width))
                        .sum::<usize>();
                    state.client.lyrics.scroll =
                        target_row.saturating_sub(usize::from(chunks[1].height) / 2);
                }
            }
            let lines = source
                .lines
                .iter()
                .enumerate()
                .map(|(index, line)| {
                    let is_active = active == Some(index);
                    let marker = if is_active { "▶ " } else { "  " };
                    let style = if is_active {
                        Style::default()
                            .fg(colors.playing)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(colors.highlight_fg)
                    };
                    Line::from(vec![
                        Span::styled(marker, style),
                        Span::styled(line.value.clone(), style),
                    ])
                })
                .collect::<Vec<_>>();
            let visual_rows = source
                .lines
                .iter()
                .map(|line| wrapped_line_height(&line.value, body_width))
                .sum::<usize>();
            let max_scroll = visual_rows.saturating_sub(usize::from(chunks[1].height));
            state.client.lyrics.scroll = state.client.lyrics.scroll.min(max_scroll);
            frame.render_widget(
                Paragraph::new(lines)
                    .scroll((crate::num::u16_sat(state.client.lyrics.scroll), 0))
                    .wrap(Wrap { trim: false }),
                chunks[1],
            );

            let follow = if state.client.lyrics.follow {
                "on"
            } else {
                "off"
            };
            let help = if help_height == 2 {
                format!(" j/k:Scroll · f:Follow {follow} · ←/→:Source\n r:Retry · y/Esc:Close")
            } else {
                format!(" j/k: Scroll · f: Follow {follow} · r: Retry · y/Esc: Close")
            };
            frame.render_widget(
                Paragraph::new(help).style(Style::default().fg(colors.muted)),
                chunks[2],
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{active_line, estimated_line, wrapped_line_height};
    use crate::subsonic::models::{LyricLine, LyricsSource};

    #[test]
    fn synchronized_line_respects_position_and_offset() {
        let source = LyricsSource {
            display_artist: None,
            display_title: None,
            lang: Some("en".into()),
            offset: 250,
            synced: true,
            lines: vec![
                LyricLine {
                    start: Some(0),
                    value: "one".into(),
                },
                LyricLine {
                    start: Some(1_000),
                    value: "two".into(),
                },
            ],
        };
        assert_eq!(active_line(&source, 1.2), Some(0));
        assert_eq!(active_line(&source, 1.3), Some(1));
    }

    #[test]
    fn narrow_lyrics_count_wrapped_rows_for_scrolling() {
        assert_eq!(wrapped_line_height("12345678", 10), 1);
        assert_eq!(wrapped_line_height("123456789", 10), 2);
    }

    #[test]
    fn untimed_lyrics_estimate_a_follow_line_from_track_progress() {
        let source = LyricsSource {
            display_artist: None,
            display_title: None,
            lang: None,
            offset: 0,
            synced: false,
            lines: (0..20)
                .map(|index| LyricLine {
                    start: None,
                    value: format!("line {index}"),
                })
                .collect(),
        };
        assert_eq!(estimated_line(&source, 25.0, 100.0), Some(5));
        assert_eq!(estimated_line(&source, 100.0, 100.0), Some(19));
        assert_eq!(estimated_line(&source, 10.0, 0.0), None);
    }
}
