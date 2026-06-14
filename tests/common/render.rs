//! Render an AppState into a TestBackend and return the buffer as text.

use std::sync::{Arc, Mutex};

use ferrosonic::app::client_state::ClientState;
use ferrosonic::app::state::AppState;
use ferrosonic::daemon::DaemonState;
use ferrosonic::ui;
use ferrosonic::ui::cover_art::CoverArtState;
use ratatui::backend::TestBackend;
use ratatui::buffer::{Buffer, Cell};
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier};
use ratatui::Terminal;

pub fn empty_cover_art_state() -> Arc<Mutex<CoverArtState>> {
    Arc::new(Mutex::new(CoverArtState {
        picker: None,
        protocol_type: None,
        cell_size: (8, 16),
        current_id: None,
        image: None,
        protocol: None,
        chafa_cache: None,
    }))
}

pub fn render(width: u16, height: u16, daemon: &DaemonState, client: &mut ClientState) -> String {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("create test terminal");
    let cover_art = empty_cover_art_state();
    terminal
        .draw(|frame| {
            let mut bundle = AppState { daemon, client };
            ui::draw(frame, &mut bundle, &cover_art);
        })
        .expect("render frame");
    buffer_to_text(terminal.backend().buffer())
}

fn buffer_to_text(buf: &Buffer) -> String {
    let mut out = String::new();
    for y in 0..buf.area.height {
        for x in 0..buf.area.width {
            out.push_str(buf[(x, y)].symbol());
        }
        out.push('\n');
    }
    out
}

/// A rendered screen that keeps per-cell style, for assertions text
/// snapshots cannot make: selection highlight, focus colour, indicators.
pub struct StyledScreen {
    buf: Buffer,
}

/// Render an `AppState` into a `TestBackend` and keep the styled buffer.
pub fn render_styled(
    width: u16,
    height: u16,
    daemon: &DaemonState,
    client: &mut ClientState,
) -> StyledScreen {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("create test terminal");
    let cover_art = empty_cover_art_state();
    terminal
        .draw(|frame| {
            let mut bundle = AppState { daemon, client };
            ui::draw(frame, &mut bundle, &cover_art);
        })
        .expect("render frame");
    StyledScreen {
        buf: terminal.backend().buffer().clone(),
    }
}

impl StyledScreen {
    /// Screen dimensions.
    pub fn width(&self) -> u16 {
        self.buf.area.width
    }
    pub fn height(&self) -> u16 {
        self.buf.area.height
    }

    /// Cell at a coordinate.
    pub fn cell(&self, x: u16, y: u16) -> &Cell {
        &self.buf[(x, y)]
    }

    /// Symbols of one row, concatenated.
    pub fn row_text(&self, y: u16) -> String {
        (0..self.buf.area.width)
            .map(|x| self.buf[(x, y)].symbol())
            .collect()
    }

    /// Whole screen as text, one line per row.
    pub fn text(&self) -> String {
        (0..self.buf.area.height)
            .map(|y| self.row_text(y))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Y rows whose text contains `needle`.
    pub fn rows_with(&self, needle: &str) -> Vec<u16> {
        (0..self.buf.area.height)
            .filter(|&y| self.row_text(y).contains(needle))
            .collect()
    }

    /// Count cells whose foreground equals `color`.
    pub fn count_fg(&self, color: Color) -> usize {
        self.cells().filter(|c| c.fg == color).count()
    }

    /// Count cells whose background equals `color`.
    pub fn count_bg(&self, color: Color) -> usize {
        self.cells().filter(|c| c.bg == color).count()
    }

    /// Count cells carrying `modifier` (e.g. BOLD).
    pub fn count_modifier(&self, modifier: Modifier) -> usize {
        self.cells().filter(|c| c.modifier.contains(modifier)).count()
    }

    /// Count foreground-coloured cells inside a region only.
    pub fn count_fg_in(&self, region: Rect, color: Color) -> usize {
        let mut n = 0;
        for y in region.top()..region.bottom().min(self.buf.area.height) {
            for x in region.left()..region.right().min(self.buf.area.width) {
                if self.buf[(x, y)].fg == color {
                    n += 1;
                }
            }
        }
        n
    }

    fn cells(&self) -> impl Iterator<Item = &Cell> {
        let area = self.buf.area;
        (0..area.height).flat_map(move |y| (0..area.width).map(move |x| &self.buf[(x, y)]))
    }
}
