//! Invariant for every two-pane page: the right (song/detail) pane shows its
//! selection highlight only while it holds focus. Regression guard for the
//! Library + Playlists focus leak; also pins the already-correct QuickPlay pane.
//!
//! Asserted on the song-pane region: unfocused -> zero highlight_fg cells,
//! focused -> at least one. The absolute zero is deliberate (a `focused &&` ->
//! `focused ||` mutation makes the focused render highlight every row, which a
//! mere focused>unfocused differential still satisfies; zero-when-unfocused
//! kills it).

mod common;

use common::{render_styled, songs};
use ferrosonic::app::client_state::ClientState;
use ferrosonic::app::models::SongOption;
use ferrosonic::app::state::Page;
use ferrosonic::config::Config;
use ferrosonic::daemon::DaemonState;
use ratatui::layout::Rect;
use ratatui::style::Color;

const W: u16 = 100;
const H: u16 = 24;

fn base() -> (DaemonState, ClientState) {
    let mut daemon = DaemonState::new(Config::new());
    daemon.config.theme = "default".into();
    (daemon, ClientState::default())
}

fn highlight_fg(client: &ClientState) -> Color {
    client.settings_state.theme_colors().highlight_fg
}

/// Right-hand song pane, inset past the splitter at column `left`.
fn song_pane(left: u16) -> Rect {
    Rect::new(left, 1, W - left - 1, H - 6)
}

#[test]
fn library_song_pane_highlights_selection_only_when_focused() {
    // 40/60 split: right pane starts at column 40.
    let region = song_pane(41);
    let (daemon, mut client) = base();
    client.page = Page::Library;
    client.artists.songs = songs("s", 3);
    client.artists.selected_song = Some(0);
    let hl = highlight_fg(&client);

    client.artists.focus = 0;
    let unfocused = render_styled(W, H, &daemon, &mut client).count_fg_in(region, hl);
    client.artists.focus = 1;
    let focused = render_styled(W, H, &daemon, &mut client).count_fg_in(region, hl);

    assert_eq!(
        unfocused, 0,
        "library song pane must show no highlight when the tree is focused"
    );
    assert!(
        focused > 0,
        "library song pane must highlight the selection when focused"
    );
}

#[test]
fn playlists_song_pane_highlights_selection_only_when_focused() {
    let region = song_pane(41);
    let (daemon, mut client) = base();
    client.page = Page::Playlists;
    client.playlists.songs = songs("p", 3);
    client.playlists.selected_song = Some(0);
    let hl = highlight_fg(&client);

    client.playlists.focus = 0;
    let unfocused = render_styled(W, H, &daemon, &mut client).count_fg_in(region, hl);
    client.playlists.focus = 1;
    let focused = render_styled(W, H, &daemon, &mut client).count_fg_in(region, hl);

    assert_eq!(
        unfocused, 0,
        "playlists song pane must show no highlight when the list is focused"
    );
    assert!(
        focused > 0,
        "playlists song pane must highlight the selection when focused"
    );
}

#[test]
fn quickplay_song_pane_highlights_selection_only_when_focused() {
    // Left option list is Length(22): right pane starts at column 22.
    let region = song_pane(23);
    let (mut daemon, mut client) = base();
    client.page = Page::QuickPlay;
    daemon.library.starred_songs = songs("q", 3);
    client.songs.selected_option = Some(SongOption::Starred);
    client.songs.selected_index = Some(0);
    let hl = highlight_fg(&client);

    client.songs.focus = 0;
    let unfocused = render_styled(W, H, &daemon, &mut client).count_fg_in(region, hl);
    client.songs.focus = 1;
    let focused = render_styled(W, H, &daemon, &mut client).count_fg_in(region, hl);

    assert_eq!(
        unfocused, 0,
        "quickplay song pane must show no highlight when the selector is focused"
    );
    assert!(
        focused > 0,
        "quickplay song pane must highlight the selection when focused"
    );
}
