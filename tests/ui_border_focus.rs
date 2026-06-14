//! Border invariant for two-pane pages: a pane's border uses border_focused
//! only while it holds focus. Asserted on column 0 (always the left pane's left
//! edge, so no layout geometry is hardcoded): the focused render must put more
//! border_focused cells there than the unfocused render.

mod common;

use common::{render_styled, songs};
use ferrosonic::app::client_state::ClientState;
use ferrosonic::app::models::SongOption;
use ferrosonic::app::state::Page;
use ferrosonic::config::Config;
use ferrosonic::daemon::DaemonState;
use ratatui::layout::Rect;

fn base() -> (DaemonState, ClientState) {
    let mut daemon = DaemonState::new(Config::new());
    daemon.config.theme = "default".into();
    (daemon, ClientState::default())
}

fn col0(height: u16) -> Rect {
    Rect::new(0, 0, 1, height)
}

#[test]
fn library_left_border_uses_focus_colour_only_when_focused() {
    let (daemon, mut client) = base();
    client.page = Page::Library;
    client.artists.songs = songs("s", 2);
    let bf = client.settings_state.theme_colors().border_focused;
    let bu = client.settings_state.theme_colors().border_unfocused;
    assert_ne!(bf, bu, "theme must distinguish focused/unfocused borders");

    client.artists.focus = 0;
    let focused = render_styled(100, 24, &daemon, &mut client).count_fg_in(col0(24), bf);
    client.artists.focus = 1;
    let unfocused = render_styled(100, 24, &daemon, &mut client).count_fg_in(col0(24), bf);

    assert!(
        focused > unfocused,
        "left pane border must use border_focused only when focused; \
         focused={focused}, unfocused={unfocused}"
    );
}

#[test]
fn playlists_left_border_uses_focus_colour_only_when_focused() {
    let (mut daemon, mut client) = base();
    client.page = Page::Playlists;
    daemon.library.playlists = Vec::new();
    client.playlists.songs = songs("p", 2);
    let bf = client.settings_state.theme_colors().border_focused;

    client.playlists.focus = 0;
    let focused = render_styled(100, 24, &daemon, &mut client).count_fg_in(col0(24), bf);
    client.playlists.focus = 1;
    let unfocused = render_styled(100, 24, &daemon, &mut client).count_fg_in(col0(24), bf);

    assert!(
        focused > unfocused,
        "playlists left border must use border_focused only when focused; \
         focused={focused}, unfocused={unfocused}"
    );
}

#[test]
fn quickplay_left_border_uses_focus_colour_only_when_focused() {
    let (mut daemon, mut client) = base();
    client.page = Page::QuickPlay;
    daemon.library.starred_songs = songs("q", 2);
    client.songs.selected_option = Some(SongOption::Starred);
    let bf = client.settings_state.theme_colors().border_focused;

    client.songs.focus = 0;
    let focused = render_styled(100, 24, &daemon, &mut client).count_fg_in(col0(24), bf);
    client.songs.focus = 1;
    let unfocused = render_styled(100, 24, &daemon, &mut client).count_fg_in(col0(24), bf);

    assert!(
        focused > unfocused,
        "quickplay left border must use border_focused only when focused; \
         focused={focused}, unfocused={unfocused}"
    );
}
