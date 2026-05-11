//! Library page render with search_results populated across scopes.

mod common;

use common::{render, song};
use ferrosonic::app::client_state::ClientState;
use ferrosonic::app::state::{FilterScope, Page};
use ferrosonic::config::Config;
use ferrosonic::daemon::DaemonState;
use ferrosonic::subsonic::models::{Album, Artist, SearchResult3};

fn build_state() -> (DaemonState, ClientState) {
    let config = Config::new();
    let mut daemon = DaemonState::new(config);
    daemon.config.theme = "default".into();
    let client = ClientState::default();
    (daemon, client)
}

#[test]
fn search_scope_artists_renders_artist_results() {
    let (daemon, mut client) = build_state();
    client.page = Page::Library;
    client.artists.filter_active = true;
    client.artists.filter = "the".into();
    client.artists.filter_scope = FilterScope::Artists;
    client.artists.search_results = Some(SearchResult3 {
        artist: vec![Artist {
            id: "a0".into(),
            name: "The Cure".into(),
            album_count: Some(13),
            cover_art: None,
        }],
        album: vec![],
        song: vec![],
    });
    let frame = render(120, 30, &daemon, &mut client);
    assert!(
        frame.contains("The Cure") || frame.contains("Cure"),
        "artist search result must render;\n{}",
        frame
    );
}

#[test]
fn search_scope_albums_renders_album_results() {
    let (daemon, mut client) = build_state();
    client.page = Page::Library;
    client.artists.filter_active = true;
    client.artists.filter = "blue".into();
    client.artists.filter_scope = FilterScope::Albums;
    client.artists.search_results = Some(SearchResult3 {
        artist: vec![],
        album: vec![Album {
            id: "alb0".into(),
            name: "Kind of Blue".into(),
            artist: Some("Miles Davis".into()),
            artist_id: Some("a0".into()),
            cover_art: None,
            song_count: Some(9),
            duration: Some(2700),
            year: Some(1959),
            genre: None,
        }],
        song: vec![],
    });
    let frame = render(120, 30, &daemon, &mut client);
    assert!(
        frame.contains("Kind of Blue") || frame.contains("Blue"),
        "album search result must render;\n{}",
        frame
    );
}

#[test]
fn search_scope_songs_renders_song_results() {
    let (daemon, mut client) = build_state();
    client.page = Page::Library;
    client.artists.filter_active = true;
    client.artists.filter = "lull".into();
    client.artists.filter_scope = FilterScope::Songs;
    client.artists.search_results = Some(SearchResult3 {
        artist: vec![],
        album: vec![],
        song: vec![song("s0", "Lullaby"), song("s1", "Lullaby II")],
    });
    let frame = render(120, 30, &daemon, &mut client);
    assert!(
        frame.contains("Lullaby"),
        "song search result must render;\n{}",
        frame
    );
}

#[test]
fn filter_scope_slash_count_shows_in_title() {
    let (daemon, mut client) = build_state();
    client.page = Page::Library;
    client.artists.filter_active = true;
    client.artists.filter = "x".into();
    client.artists.filter_scope = FilterScope::Songs;
    let frame = render(120, 30, &daemon, &mut client);
    assert!(
        frame.contains("///"),
        "Songs scope must show 3 slashes in the title;\n{}",
        frame
    );
}
