use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Widget},
};

use crate::app::state::NowPlaying;
use crate::ui::theme::ThemeColors;

pub struct NowPlayingWidget<'a> {
    now_playing: &'a NowPlaying,
    focused: bool,
    colors: ThemeColors,
}

impl<'a> NowPlayingWidget<'a> {
    pub fn new(now_playing: &'a NowPlaying, colors: ThemeColors) -> Self {
        Self {
            now_playing,
            focused: false,
            colors,
        }
    }

    #[allow(dead_code)]
    pub fn focused(mut self, focused: bool) -> Self {
        self.focused = focused;
        self
    }
}

impl Widget for NowPlayingWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height < 4 || area.width < 20 {
            return;
        }

        let block = Block::default()
            .borders(Borders::ALL)
            .title(" Now Playing ")
            .border_style(if self.focused {
                Style::default().fg(self.colors.border_focused)
            } else {
                Style::default().fg(self.colors.border_unfocused)
            });

        let inner = block.inner(area);
        block.render(area, buf);

        if inner.height < 2 {
            return;
        }

        if self.now_playing.song.is_none() {
            let no_track = Paragraph::new("No track playing")
                .style(Style::default().fg(self.colors.muted))
                .alignment(Alignment::Center);
            no_track.render(inner, buf);
            return;
        }

        let song = self.now_playing.song.as_ref().unwrap();

        let artist = song.artist.clone().unwrap_or_default();
        let album = song.album.clone().unwrap_or_default();
        let title = song.title.clone();

        let mut quality_parts = Vec::new();
        if let Some(ref fmt) = self.now_playing.format {
            quality_parts.push(fmt.to_string().to_uppercase());
        }
        if let Some(bits) = self.now_playing.bit_depth {
            quality_parts.push(format!("{}-bit", bits));
        }
        if let Some(rate) = self.now_playing.sample_rate {
            let khz = rate as f64 / 1000.0;
            if khz == khz.floor() {
                quality_parts.push(format!("{}kHz", khz as u32));
            } else {
                quality_parts.push(format!("{:.1}kHz", khz));
            }
        }
        if let Some(ref channels) = self.now_playing.channels {
            quality_parts.push(channels.to_string());
        }
        let quality = quality_parts.join(" │ ");

        if inner.height >= 5 {
            let chunks = Layout::vertical([
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Length(1),
            ])
            .split(inner);

            let artist_line = Line::from(vec![Span::styled(
                &artist,
                Style::default().fg(self.colors.artist),
            )]);
            Paragraph::new(artist_line)
                .alignment(Alignment::Center)
                .render(chunks[0], buf);

            let album_line = Line::from(vec![Span::styled(
                &album,
                Style::default().fg(self.colors.album),
            )]);
            Paragraph::new(album_line)
                .alignment(Alignment::Center)
                .render(chunks[1], buf);

            let title_line = Line::from(vec![Span::styled(
                &title,
                Style::default()
                    .fg(self.colors.highlight_fg)
                    .add_modifier(Modifier::BOLD),
            )]);
            Paragraph::new(title_line)
                .alignment(Alignment::Center)
                .render(chunks[2], buf);

            if !quality.is_empty() {
                let quality_line = Line::from(vec![Span::styled(
                    &quality,
                    Style::default().fg(self.colors.muted),
                )]);
                Paragraph::new(quality_line)
                    .alignment(Alignment::Center)
                    .render(chunks[3], buf);
            }

            render_progress_bar(
                chunks[4],
                buf,
                self.now_playing.progress_percent(),
                &self.now_playing.format_position(),
                &self.now_playing.format_duration(),
                &self.colors,
            );
        } else if inner.height >= 3 {
            let chunks = Layout::vertical([
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Length(1),
            ])
            .split(inner);

            let line1 = Line::from(vec![
                Span::styled(
                    &title,
                    Style::default()
                        .fg(self.colors.highlight_fg)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(" - ", Style::default().fg(self.colors.muted)),
                Span::styled(&artist, Style::default().fg(self.colors.artist)),
            ]);
            Paragraph::new(line1)
                .alignment(Alignment::Center)
                .render(chunks[0], buf);

            let line2 = Line::from(vec![Span::styled(
                &album,
                Style::default().fg(self.colors.album),
            )]);
            Paragraph::new(line2)
                .alignment(Alignment::Center)
                .render(chunks[1], buf);

            render_progress_bar(
                chunks[2],
                buf,
                self.now_playing.progress_percent(),
                &self.now_playing.format_position(),
                &self.now_playing.format_duration(),
                &self.colors,
            );
        } else {
            let chunks = Layout::vertical([
                Constraint::Length(1),
                Constraint::Length(1),
            ])
            .split(inner);

            let line1 = Line::from(vec![Span::styled(
                &title,
                Style::default().fg(self.colors.highlight_fg),
            )]);
            Paragraph::new(line1)
                .alignment(Alignment::Center)
                .render(chunks[0], buf);

            render_progress_bar(
                chunks[1],
                buf,
                self.now_playing.progress_percent(),
                &self.now_playing.format_position(),
                &self.now_playing.format_duration(),
                &self.colors,
            );
        }
    }
}

fn render_progress_bar(
    area: Rect,
    buf: &mut Buffer,
    progress: f64,
    pos: &str,
    dur: &str,
    colors: &ThemeColors,
) {
    if area.width < 15 {
        return;
    }

    let time_str = format!("{} / {}", pos, dur);
    let time_width = time_str.len() as u16;

    let bar_width = area.width.saturating_sub(time_width + 3);
    let total_width = time_width + 2 + bar_width;
    let start_x = area.x + (area.width.saturating_sub(total_width)) / 2;

    buf.set_string(
        start_x,
        area.y,
        &time_str,
        Style::default().fg(colors.highlight_fg),
    );

    let bar_start = start_x + time_width + 2;
    if bar_width > 0 {
        let filled = (bar_width as f64 * progress) as u16;

        for x in bar_start..(bar_start + filled) {
            buf[(x, area.y)]
                .set_char('━')
                .set_style(Style::default().fg(colors.success));
        }

        for x in (bar_start + filled)..(bar_start + bar_width) {
            buf[(x, area.y)]
                .set_char('─')
                .set_style(Style::default().fg(colors.muted));
        }
    }
}
