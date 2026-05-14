//! Cava audio visualizer widget — renders captured noncurses output

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    widgets::Widget,
};

use crate::app::state::{CavaColor, CavaRow};

pub struct CavaWidget<'a> {
    screen: &'a [CavaRow],
}

impl<'a> CavaWidget<'a> {
    pub fn new(screen: &'a [CavaRow]) -> Self {
        Self { screen }
    }
}

fn cava_color_to_ratatui(c: CavaColor) -> Option<Color> {
    match c {
        CavaColor::Default => None,
        CavaColor::Indexed(i) => Some(Color::Indexed(i)),
        CavaColor::Rgb(r, g, b) => Some(Color::Rgb(r, g, b)),
    }
}

impl Widget for CavaWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 || self.screen.is_empty() {
            return;
        }

        for (row_idx, cava_row) in self.screen.iter().enumerate() {
            if row_idx >= area.height as usize {
                break;
            }
            let y = area.y + row_idx as u16;
            let mut x = area.x;

            for span in &cava_row.spans {
                for ch in span.text.chars() {
                    if x >= area.x + area.width {
                        break;
                    }
                    let mut style = Style::default();
                    if let Some(fg) = cava_color_to_ratatui(span.fg) {
                        style = style.fg(fg);
                    }
                    if let Some(bg) = cava_color_to_ratatui(span.bg) {
                        style = style.bg(bg);
                    }
                    buf[(x, y)].set_char(ch).set_style(style);
                    x += 1;
                }
            }
        }
    }
}
