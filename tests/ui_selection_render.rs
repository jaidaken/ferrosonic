//! Selection invariant: the highlight lands on the selected row, not another.
//! Kills the `selected == i` -> `selected != i` render mutants (which move the
//! highlight to every other row). Asserts the selected title's row carries
//! highlight_fg and a non-selected title's row does not.

mod common;

use common::{render_styled, song};
use ferrosonic::app::client_state::ClientState;
use ferrosonic::app::models::SongOption;
use ferrosonic::app::state::Page;
use ferrosonic::config::Config;
use ferrosonic::daemon::DaemonState;
use ferrosonic::subsonic::models::Playlist;
use ratatui::style::Color;

const W: u16 = 120;
const H: u16 = 30;

fn base() -> (DaemonState, ClientState) {
    let mut daemon = DaemonState::new(Config::new());
    daemon.config.theme = "default".into();
    (daemon, ClientState::default())
}

fn three() -> Vec<ferrosonic::subsonic::models::Child> {
    vec![
        song("a", "AlphaTrack"),
        song("b", "BetaTrack"),
        song("c", "GammaTrack"),
    ]
}

fn assert_selected_row(
    screen: &common::StyledScreen,
    cols: (u16, u16),
    color: Color,
    selected_title: &str,
    other_title: &str,
) {
    let (x0, x1) = cols;
    let sel = screen.row_of(selected_title).unwrap_or_else(|| {
        panic!(
            "selected title {selected_title} not rendered\n{}",
            screen.text()
        )
    });
    let other = screen
        .row_of(other_title)
        .unwrap_or_else(|| panic!("other title {other_title} not rendered\n{}", screen.text()));
    assert!(
        screen.row_has_fg_in(sel, x0, x1, color),
        "selected row ({selected_title}) must be highlighted\n{}",
        screen.text()
    );
    assert!(
        !screen.row_has_fg_in(other, x0, x1, color),
        "non-selected row ({other_title}) must not be highlighted\n{}",
        screen.text()
    );
}

#[test]
fn library_song_pane_highlights_the_selected_row() {
    let (daemon, mut client) = base();
    client.page = Page::Library;
    client.artists.songs = three();
    client.artists.selected_song = Some(1);
    client.artists.focus = 1;
    let hl = client.settings_state.theme_colors().highlight_fg;
    let screen = render_styled(W, H, &daemon, &mut client);
    assert_selected_row(&screen, (41, W), hl, "BetaTrack", "AlphaTrack");
}

#[test]
fn playlists_song_pane_highlights_the_selected_row() {
    let (daemon, mut client) = base();
    client.page = Page::Playlists;
    client.playlists.songs = three();
    client.playlists.selected_song = Some(1);
    client.playlists.focus = 1;
    let hl = client.settings_state.theme_colors().highlight_fg;
    let screen = render_styled(W, H, &daemon, &mut client);
    assert_selected_row(&screen, (41, W), hl, "BetaTrack", "AlphaTrack");
}

#[test]
fn quickplay_song_pane_highlights_the_selected_row() {
    let (mut daemon, mut client) = base();
    client.page = Page::QuickPlay;
    daemon.library.starred_songs = three();
    client.songs.selected_option = Some(SongOption::Starred);
    client.songs.selected_index = Some(1);
    client.songs.focus = 1;
    let hl = client.settings_state.theme_colors().highlight_fg;
    let screen = render_styled(W, H, &daemon, &mut client);
    assert_selected_row(&screen, (23, W), hl, "BetaTrack", "AlphaTrack");
}

#[test]
fn playlists_list_highlights_the_selected_playlist() {
    let (mut daemon, mut client) = base();
    client.page = Page::Playlists;
    daemon.library.playlists = vec![
        playlist("p0", "AlphaList"),
        playlist("p1", "BetaList"),
        playlist("p2", "GammaList"),
    ];
    client.playlists.selected_playlist = Some(1);
    client.playlists.focus = 0;
    // The playlist list marks the selection with `primary` + bold, not highlight_fg.
    let primary = client.settings_state.theme_colors().primary;
    let screen = render_styled(W, H, &daemon, &mut client);
    assert_selected_row(&screen, (1, 40), primary, "BetaList", "AlphaList");
}

fn playlist(id: &str, name: &str) -> Playlist {
    Playlist {
        id: id.into(),
        name: name.into(),
        owner: None,
        song_count: Some(3),
        duration: Some(600),
        cover_art: None,
        public: None,
        comment: None,
    }
}
