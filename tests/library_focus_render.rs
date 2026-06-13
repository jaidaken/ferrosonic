//! Regression: the Library song pane shows its selection highlight only when
//! it has focus, never while the artist/album tree (focus 0) is active.

mod common;

use common::render::empty_cover_art_state;
use common::songs;
use ferrosonic::app::client_state::ClientState;
use ferrosonic::app::state::{AppState, Page};
use ferrosonic::config::Config;
use ferrosonic::daemon::DaemonState;
use ferrosonic::ui;
use ratatui::backend::TestBackend;
use ratatui::style::Color;
use ratatui::Terminal;

fn count_fg(client: &mut ClientState, daemon: &DaemonState, target: Color) -> usize {
    let backend = TestBackend::new(100, 24);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    let cover_art = empty_cover_art_state();
    terminal
        .draw(|frame| {
            let mut bundle = AppState { daemon, client };
            ui::draw(frame, &mut bundle, &cover_art);
        })
        .expect("draw frame");
    let buf = terminal.backend().buffer();
    let mut n = 0;
    for y in 0..buf.area.height {
        for x in 0..buf.area.width {
            if buf[(x, y)].fg == target {
                n += 1;
            }
        }
    }
    n
}

fn build() -> (DaemonState, ClientState) {
    let mut daemon = DaemonState::new(Config::new());
    daemon.config.theme = "default".into();
    let mut client = ClientState::default();
    client.page = Page::Library;
    client.artists.songs = songs("s", 3);
    client.artists.selected_song = Some(0);
    (daemon, client)
}

#[test]
fn song_pane_highlights_selection_only_when_focused() {
    let target = build().1.settings_state.theme_colors().highlight_fg;

    let (daemon, mut client) = build();

    client.artists.focus = 0;
    let tree_focused = count_fg(&mut client, &daemon, target);

    client.artists.focus = 1;
    let song_focused = count_fg(&mut client, &daemon, target);

    assert!(
        song_focused > tree_focused,
        "the selected song must use highlight_fg only when the song pane is \
         focused; tree-focused cells={tree_focused}, song-focused={song_focused}"
    );
}
