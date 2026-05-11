//! Render an AppState into a TestBackend and return the buffer as text.

use std::sync::{Arc, Mutex};

use ferrosonic::app::client_state::ClientState;
use ferrosonic::app::state::AppState;
use ferrosonic::daemon::DaemonState;
use ferrosonic::ui;
use ferrosonic::ui::cover_art::CoverArtState;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
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
