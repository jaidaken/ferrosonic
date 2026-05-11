use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    widgets::Widget,
};

#[allow(dead_code)]
pub struct ProgressBar<'a> {
    progress: f64,
    position_text: &'a str,
    duration_text: &'a str,
    filled_style: Style,
    empty_style: Style,
    text_style: Style,
}

#[allow(dead_code)]
impl<'a> ProgressBar<'a> {
    pub fn new(progress: f64, position_text: &'a str, duration_text: &'a str) -> Self {
        Self {
            progress: progress.clamp(0.0, 1.0),
            position_text,
            duration_text,
            filled_style: Style::default().bg(Color::Blue),
            empty_style: Style::default().bg(Color::DarkGray),
            text_style: Style::default().fg(Color::White),
        }
    }

    pub fn filled_style(mut self, style: Style) -> Self {
        self.filled_style = style;
        self
    }

    pub fn empty_style(mut self, style: Style) -> Self {
        self.empty_style = style;
        self
    }

    pub fn text_style(mut self, style: Style) -> Self {
        self.text_style = style;
        self
    }

    /// Maps mouse `x` to a 0.0–1.0 position; `None` outside the bar.
    pub fn position_from_x(area: Rect, x: u16) -> Option<f64> {
        let bar_start = area.x + 8;
        let bar_end = area.x + area.width - 8;

        if x >= bar_start && x < bar_end {
            let bar_width = bar_end - bar_start;
            let relative_x = x - bar_start;
            Some(relative_x as f64 / bar_width as f64)
        } else {
            None
        }
    }
}

impl Widget for ProgressBar<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width < 20 || area.height < 1 {
            return;
        }

        let pos_width = self.position_text.len();
        let dur_width = self.duration_text.len();

        buf.set_string(area.x, area.y, self.position_text, self.text_style);

        let dur_x = area.x + area.width - dur_width as u16;
        buf.set_string(dur_x, area.y, self.duration_text, self.text_style);

        let bar_x = area.x + pos_width as u16 + 1;
        let bar_width = area
            .width
            .saturating_sub((pos_width + dur_width + 2) as u16);

        if bar_width > 0 {
            let filled_width = (bar_width as f64 * self.progress) as u16;

            for x in bar_x..(bar_x + filled_width) {
                buf[(x, area.y)].set_char('━').set_style(self.filled_style);
            }

            for x in (bar_x + filled_width)..(bar_x + bar_width) {
                buf[(x, area.y)].set_char('─').set_style(self.empty_style);
            }
        }
    }
}

#[allow(dead_code)]
pub struct VerticalBar {
    value: f64,
    filled_style: Style,
    empty_style: Style,
}

#[allow(dead_code)]
impl VerticalBar {
    pub fn new(value: f64) -> Self {
        Self {
            value: value.clamp(0.0, 1.0),
            filled_style: Style::default().bg(Color::Blue),
            empty_style: Style::default().bg(Color::DarkGray),
        }
    }

    pub fn filled_style(mut self, style: Style) -> Self {
        self.filled_style = style;
        self
    }

    pub fn empty_style(mut self, style: Style) -> Self {
        self.empty_style = style;
        self
    }
}

impl Widget for VerticalBar {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height < 1 || area.width < 1 {
            return;
        }

        let filled_height = (area.height as f64 * self.value) as u16;
        let empty_start = area.y + area.height - filled_height;

        for y in area.y..empty_start {
            for x in area.x..(area.x + area.width) {
                buf[(x, y)].set_char('░').set_style(self.empty_style);
            }
        }

        for y in empty_start..(area.y + area.height) {
            for x in area.x..(area.x + area.width) {
                buf[(x, y)].set_char('█').set_style(self.filled_style);
            }
        }
    }
}
