//! Artist/album information overlay.
//!
//! Navidrome only returns this data with a server-side external (Last.fm)
//! integration; otherwise the panel shows a clean empty state.

use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    Frame,
};

use crate::app::page_state::{InfoPayload, InfoStatus};
use crate::app::state::AppState;
use crate::ui::theme::ThemeColors;

/// Label for the described entity.
const fn kind_label(state: &AppState<'_>) -> &'static str {
    match state.client.info.kind {
        crate::app::page_state::InfoKind::Artist => "Artist",
        crate::app::page_state::InfoKind::Album => "Album",
    }
}

/// Body lines for a ready payload. External URLs and IDs render after the
/// prose so a long biography does not bury them.
fn paragraph_lines(text: &str, colors: &ThemeColors) -> Vec<Line<'static>> {
    text.split('\n')
        .map(|chunk| {
            Line::from(Span::styled(
                chunk.to_string(),
                Style::default().fg(colors.highlight_fg),
            ))
        })
        .collect()
}

fn link_line(label: &str, value: &str, colors: &ThemeColors) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{label}: "), Style::default().fg(colors.accent)),
        Span::styled(value.to_string(), Style::default().fg(colors.muted)),
    ])
}

fn body_lines(payload: &InfoPayload, colors: &ThemeColors) -> Vec<Line<'static>> {
    let mut lines: Vec<Line<'static>> = Vec::new();
    let (prose, last_fm, mb) = match payload {
        InfoPayload::Artist(info) => (
            info.biography.as_deref(),
            info.last_fm_url.as_deref(),
            info.music_brainz_id.as_deref(),
        ),
        InfoPayload::Album(info) => (
            info.notes.as_deref(),
            info.last_fm_url.as_deref(),
            info.music_brainz_id.as_deref(),
        ),
    };
    if let Some(prose) = prose.filter(|p| !p.trim().is_empty()) {
        lines.extend(paragraph_lines(prose, colors));
    }
    if let InfoPayload::Artist(info) = payload {
        if !info.similar_artist.is_empty() {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "Similar artists:",
                Style::default().fg(colors.accent),
            )));
            for artist in &info.similar_artist {
                lines.push(Line::from(Span::styled(
                    format!("  • {}", artist.name),
                    Style::default().fg(colors.highlight_fg),
                )));
            }
        }
    }
    if let Some(url) = last_fm {
        lines.push(link_line("Last.fm", url, colors));
    }
    if let Some(id) = mb {
        lines.push(link_line("MusicBrainz", id, colors));
    }
    lines
}

/// Draw the info overlay over the completed application frame.
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
    let label = kind_label(state);
    let title = if state.client.info.title.is_empty() {
        label.to_string()
    } else {
        state.client.info.title.clone()
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(colors.accent))
        .title(format!(" {label} — {title} "));
    let inner = block.inner(rect);

    frame.render_widget(Clear, rect);
    frame.render_widget(block, rect);

    match &state.client.info.status {
        InfoStatus::Idle | InfoStatus::Loading => {
            frame.render_widget(Paragraph::new("Loading information…").centered(), inner);
        }
        InfoStatus::Empty => {
            frame.render_widget(
                Paragraph::new(format!(
                    "No information available for this {}.\n\nThis is common unless your server has an\nLast.fm integration configured.\n\nr: Retry   I/Esc: Close",
                    label.to_lowercase()
                ))
                .centered()
                .wrap(Wrap { trim: true }),
                inner,
            );
        }
        InfoStatus::Error(error) => {
            frame.render_widget(
                Paragraph::new(format!(
                    "Could not load information\n\n{error}\n\nr: Retry   I/Esc: Close"
                ))
                .style(Style::default().fg(colors.error))
                .centered()
                .wrap(Wrap { trim: false }),
                inner,
            );
        }
        InfoStatus::Ready(payload) => {
            let chunks = Layout::vertical([Constraint::Min(1), Constraint::Length(2)]).split(inner);
            let mut lines = body_lines(payload, colors);
            lines.push(Line::from(""));
            let body_height = usize::from(chunks[0].height);
            let max_scroll = lines.len().saturating_sub(body_height);
            state.client.info.scroll = state.client.info.scroll.min(max_scroll);
            frame.render_widget(
                Paragraph::new(lines)
                    .scroll((crate::num::u16_sat(state.client.info.scroll), 0))
                    .wrap(Wrap { trim: false }),
                chunks[0],
            );
            frame.render_widget(
                Paragraph::new(" j/k: Scroll · r: Retry · I/Esc: Close")
                    .style(Style::default().fg(colors.muted)),
                chunks[1],
            );
        }
    }
}
