//! Invariant for every two-pane page: the right (song/detail) pane shows its
//! selection highlight only while it holds focus, never while the left pane is
//! active. Regression guard for the Library + Playlists focus leak (fixed in
//! the focus-gate commit); also pins the already-correct QuickPlay pane so it
//! cannot regress the same way.
//!
//! The assertion is a focus differential: count highlight_fg cells with the
//! left pane focused vs the right pane focused. The selected song must add
//! highlight_fg cells only in the focused render. Under the bug both renders
//! highlight the same row, so the counts are equal and the assert fails.

mod common;

use common::{render_styled, songs};
use ferrosonic::app::client_state::ClientState;
use ferrosonic::app::models::SongOption;
use ferrosonic::app::state::Page;
use ferrosonic::config::Config;
use ferrosonic::daemon::DaemonState;
use ratatui::style::Color;

fn base() -> (DaemonState, ClientState) {
    let mut daemon = DaemonState::new(Config::new());
    daemon.config.theme = "default".into();
    (daemon, ClientState::default())
}

fn highlight_fg(client: &ClientState) -> Color {
    client.settings_state.theme_colors().highlight_fg
}

#[test]
fn library_song_pane_highlights_only_when_focused() {
    let (daemon, mut client) = base();
    client.page = Page::Library;
    client.artists.songs = songs("s", 3);
    client.artists.selected_song = Some(0);
    let target = highlight_fg(&client);

    client.artists.focus = 0;
    let unfocused = render_styled(100, 24, &daemon, &mut client).count_fg(target);
    client.artists.focus = 1;
    let focused = render_styled(100, 24, &daemon, &mut client).count_fg(target);

    assert!(
        focused > unfocused,
        "library song pane must highlight only when focused; \
         left-focused={unfocused}, song-focused={focused}"
    );
}

#[test]
fn playlists_song_pane_highlights_only_when_focused() {
    let (daemon, mut client) = base();
    client.page = Page::Playlists;
    client.playlists.songs = songs("p", 3);
    client.playlists.selected_song = Some(0);
    let target = highlight_fg(&client);

    client.playlists.focus = 0;
    let unfocused = render_styled(100, 24, &daemon, &mut client).count_fg(target);
    client.playlists.focus = 1;
    let focused = render_styled(100, 24, &daemon, &mut client).count_fg(target);

    assert!(
        focused > unfocused,
        "playlists song pane must highlight only when focused; \
         left-focused={unfocused}, song-focused={focused}"
    );
}

#[test]
fn quickplay_song_pane_highlights_only_when_focused() {
    let (mut daemon, mut client) = base();
    client.page = Page::QuickPlay;
    daemon.library.starred_songs = songs("q", 3);
    client.songs.selected_option = Some(SongOption::Starred);
    client.songs.selected_index = Some(0);
    let target = highlight_fg(&client);

    client.songs.focus = 0;
    let unfocused = render_styled(100, 24, &daemon, &mut client).count_fg(target);
    client.songs.focus = 1;
    let focused = render_styled(100, 24, &daemon, &mut client).count_fg(target);

    assert!(
        focused > unfocused,
        "quickplay song pane must highlight only when focused; \
         left-focused={unfocused}, song-focused={focused}"
    );
}
