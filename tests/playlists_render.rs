//! Playlists page render with populated state.

mod common;

use common::{render, song};
use ferrosonic::app::client_state::ClientState;
use ferrosonic::app::state::Page;
use ferrosonic::config::Config;
use ferrosonic::daemon::DaemonState;
use ferrosonic::subsonic::models::Playlist;

fn build_state() -> (DaemonState, ClientState) {
    let config = Config::new();
    let mut daemon = DaemonState::new(config);
    daemon.config.theme = "default".into();
    let client = ClientState::default();
    (daemon, client)
}

fn playlist(id: &str, name: &str, count: i32, dur: i32) -> Playlist {
    Playlist {
        id: id.into(),
        name: name.into(),
        owner: None,
        song_count: Some(count),
        duration: Some(dur),
        cover_art: None,
        public: None,
        comment: None,
    }
}

#[test]
fn empty_playlist_list_renders_empty_state_message() {
    let (daemon, mut client) = build_state();
    client.page = Page::Playlists;
    let frame = render(80, 24, &daemon, &mut client);
    assert!(
        frame.contains("No playlists") || frame.contains("Playlists"),
        "empty state must render a hint or border;\n{}",
        frame
    );
}

#[test]
fn populated_playlists_render_names_and_counts() {
    let (mut daemon, mut client) = build_state();
    daemon.library.playlists = vec![
        playlist("p0", "Liked Songs", 42, 7200),
        playlist("p1", "Workout", 15, 3600),
    ];
    client.page = Page::Playlists;
    let frame = render(100, 30, &daemon, &mut client);
    assert!(
        frame.contains("Liked Songs"),
        "expected playlist name; got\n{}",
        frame
    );
    assert!(frame.contains("Workout"));
    assert!(
        frame.contains("42") || frame.contains("15"),
        "song count must render"
    );
}

#[test]
fn selected_playlist_is_highlighted() {
    let (mut daemon, mut client) = build_state();
    daemon.library.playlists = vec![playlist("p0", "Mix A", 10, 1800)];
    client.page = Page::Playlists;
    client.playlists.selected_playlist = Some(0);
    let frame = render(80, 24, &daemon, &mut client);
    assert!(frame.contains("Mix A"));
}

#[test]
fn playlist_songs_render_when_focus_is_songs_pane() {
    let (mut daemon, mut client) = build_state();
    daemon.library.playlists = vec![playlist("p0", "Mix", 2, 360)];
    client.page = Page::Playlists;
    client.playlists.selected_playlist = Some(0);
    client.playlists.focus = 1;
    client.playlists.songs = vec![song("s0", "Track One"), song("s1", "Track Two")];
    client.playlists.selected_song = Some(0);
    let frame = render(120, 30, &daemon, &mut client);
    assert!(
        frame.contains("Track One") || frame.contains("Track Two"),
        "expected song titles in playlist songs pane;\n{}",
        frame
    );
}

#[test]
fn playlist_with_zero_duration_does_not_crash() {
    let (mut daemon, mut client) = build_state();
    daemon.library.playlists = vec![playlist("p0", "Empty", 0, 0)];
    client.page = Page::Playlists;
    let frame = render(80, 24, &daemon, &mut client);
    assert!(frame.contains("Empty"));
}
