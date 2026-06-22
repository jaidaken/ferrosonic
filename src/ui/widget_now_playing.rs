//! Now-playing strip widget with progress bar and cover art.

use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Widget},
};

use crate::daemon::state::NowPlaying;
use crate::ui::theme::ThemeColors;

/// Now-playing strip: title, artist, format info, progress bar.
pub struct NowPlayingWidget<'a> {
    now_playing: &'a NowPlaying,
    focused: bool,
    colors: ThemeColors,
    /// Reserve this many cols on the right of the info area so the
    /// caller can render cover art there. Progress bar still spans
    /// the full inner width below the reserved region.
    art_reserved_cols: u16,
}

impl<'a> NowPlayingWidget<'a> {
    /// Widget over the current now-playing state.
    #[must_use]
    pub const fn new(now_playing: &'a NowPlaying, colors: ThemeColors) -> Self {
        Self {
            now_playing,
            focused: false,
            colors,
            art_reserved_cols: 0,
        }
    }

    /// Builder: mark the pane focused for border styling.
    #[must_use]
    pub const fn focused(mut self, focused: bool) -> Self {
        self.focused = focused;
        self
    }

    /// Builder: reserve columns on the left for cover art.
    #[must_use]
    pub const fn art_reserved_cols(mut self, cols: u16) -> Self {
        self.art_reserved_cols = cols;
        self
    }
}

/// Largest visually-square rect that fits inside the right-half reservation,
/// centered.
///
/// `cell_size` is the pixel dimensions of one terminal cell; we choose
/// `art_w` / `art_h` so `art_w * cell.0 == art_h * cell.1` (rendered pixels
/// match → square cover).
#[must_use]
pub fn art_rect(area: Rect, cover_art_cols: u16, cell_size: (u16, u16)) -> Option<Rect> {
    if cover_art_cols == 0 || area.height < 4 || area.width < cover_art_cols + 20 {
        return None;
    }
    let inner = Block::default().borders(Borders::ALL).inner(area);
    if inner.height < 2 {
        return None;
    }
    let right_x = inner.x + inner.width - cover_art_cols;
    let right_w = cover_art_cols;
    let right_h = inner.height.saturating_sub(1);

    let (cw, ch) = (u32::from(cell_size.0.max(1)), u32::from(cell_size.1.max(1)));
    // For visually square output: art_w/art_h = ch/cw.
    let art_h = u32::from(right_h).min(u32::from(right_w) * cw / ch);
    let art_w = art_h * ch / cw;
    if art_w == 0 || art_h == 0 {
        return None;
    }
    let art_w = crate::num::u16_sat(art_w);
    let art_h = crate::num::u16_sat(art_h);
    let pad_x = (right_w - art_w) / 2;
    let pad_y = (right_h - art_h) / 2;
    Some(Rect::new(right_x + pad_x, inner.y + pad_y, art_w, art_h))
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

        // Info area sits above the progress bar (last row of inner)
        // and to the left of any reserved cover-art region.
        let info_h = inner.height.saturating_sub(1);
        let info_w = inner.width.saturating_sub(self.art_reserved_cols);
        let info_area = Rect::new(inner.x, inner.y, info_w, info_h);
        let progress_area = Rect::new(inner.x, inner.y + info_h, inner.width, 1);

        let Some(song) = self.now_playing.song.as_ref() else {
            let no_track = Paragraph::new("No track playing")
                .style(Style::default().fg(self.colors.muted))
                .alignment(Alignment::Center);
            no_track.render(info_area, buf);
            return;
        };

        let artist = song.artist.clone().unwrap_or_default();
        let album = song.album.clone().unwrap_or_default();
        let title = song.title.clone();
        let quality = build_quality_string(self.now_playing);

        render_info(
            info_area,
            buf,
            &artist,
            &album,
            &title,
            &quality,
            &self.colors,
        );

        render_progress_bar(
            progress_area,
            buf,
            self.now_playing.progress_percent(),
            &self.now_playing.format_position(),
            &self.now_playing.format_duration(),
            &self.colors,
        );
    }
}

fn build_quality_string(np: &NowPlaying) -> String {
    let mut parts = Vec::new();
    if let Some(ref fmt) = np.format {
        parts.push(fmt.clone().to_uppercase());
    }
    if let Some(bits) = np.bit_depth {
        parts.push(format!("{bits}-bit"));
    }
    if let Some(rate) = np.sample_rate {
        let khz = f64::from(rate) / 1000.0;
        // Exact integer-value check; floor() is exact, an epsilon compare would be wrong.
        #[allow(clippy::float_cmp)]
        let is_whole_khz = khz == khz.floor();
        if is_whole_khz {
            // f64->u32 `as` saturates; khz is a positive sample-rate/1000.
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let khz_int = khz as u32;
            parts.push(format!("{khz_int}kHz"));
        } else {
            parts.push(format!("{khz:.1}kHz"));
        }
    }
    if let Some(ref channels) = np.channels {
        parts.push(channels.clone());
    }
    parts.join(" │ ")
}

fn render_info(
    area: Rect,
    buf: &mut Buffer,
    artist: &str,
    album: &str,
    title: &str,
    quality: &str,
    colors: &ThemeColors,
) {
    if area.width < 4 || area.height < 1 {
        return;
    }

    // Choose layout based on available height; centre vertically by
    // padding above/below with empty Min rows.
    let (lines, styles): (Vec<String>, Vec<Style>) = if area.height >= 4 {
        (
            vec![artist.into(), album.into(), title.into(), quality.into()],
            vec![
                Style::default().fg(colors.artist),
                Style::default().fg(colors.album),
                Style::default()
                    .fg(colors.highlight_fg)
                    .add_modifier(Modifier::BOLD),
                Style::default().fg(colors.muted),
            ],
        )
    } else if area.height >= 3 {
        (
            vec![
                format!("{} — {}", title, artist),
                album.into(),
                quality.into(),
            ],
            vec![
                Style::default()
                    .fg(colors.highlight_fg)
                    .add_modifier(Modifier::BOLD),
                Style::default().fg(colors.album),
                Style::default().fg(colors.muted),
            ],
        )
    } else if area.height >= 2 {
        (
            vec![title.into(), format!("{} — {}", artist, album)],
            vec![
                Style::default()
                    .fg(colors.highlight_fg)
                    .add_modifier(Modifier::BOLD),
                Style::default().fg(colors.muted),
            ],
        )
    } else {
        (
            vec![title.into()],
            vec![Style::default().fg(colors.highlight_fg)],
        )
    };

    let n = crate::num::u16_sat(lines.len());
    let pad = area.height.saturating_sub(n) / 2;
    for (i, (text, style)) in lines.iter().zip(styles.iter()).enumerate() {
        let y = area.y + pad + crate::num::u16_sat(i);
        if y >= area.y + area.height {
            break;
        }
        let row = Rect::new(area.x, y, area.width, 1);
        Paragraph::new(Line::from(vec![Span::styled(text.clone(), *style)]))
            .alignment(Alignment::Center)
            .render(row, buf);
    }
}

/// Paint the playback progress bar into `area`.
pub fn render_progress_bar(
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

    let time_str = format!("{pos} / {dur}");
    let time_width = crate::num::u16_sat(time_str.len());

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
        // f64->u16 `as` saturates; bar_width*progress(0.0..=1.0) is bounded by bar_width.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let filled = (f64::from(bar_width) * progress) as u16;

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
