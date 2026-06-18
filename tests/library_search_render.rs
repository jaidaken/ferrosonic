//! Library page render with search_results populated across scopes.

mod common;

use common::{render, song};
use ferrosonic::app::client_state::ClientState;
use ferrosonic::app::state::Page;
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
    client.artists.search_results = Some(SearchResult3 {
        artist: vec![],
        album: vec![Album {
            id: "alb0".into(),
            name: "Kind of Blue".into(),
            artist: Some("Miles Davis".into()),
            artist_id: Some("a0".into()),
            cover_art: None,
            song_count: Some(9),
            original_release_date: None,
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
fn unified_search_shows_artists_albums_and_songs_at_once() {
    // One search returns all kinds (no scope). Albums group under one greyed
    // parent-artist label; that label is the only place its name appears.
    let (daemon, mut client) = build_state();
    client.page = Page::Library;
    client.artists.filter_active = true;
    client.artists.filter = "b".into();
    let alb = |id: &str, name: &str| Album {
        id: id.into(),
        name: name.into(),
        artist: Some("David Bowie".into()),
        artist_id: Some("a-bowie".into()),
        cover_art: None,
        song_count: Some(10),
        original_release_date: None,
        duration: Some(2000),
        year: Some(1977),
        genre: None,
    };
    client.artists.search_results = Some(SearchResult3 {
        artist: vec![Artist {
            id: "a-blur".into(),
            name: "Blur".into(),
            album_count: Some(2),
            cover_art: None,
        }],
        album: vec![alb("alb1", "Low"), alb("alb2", "Heroes")],
        song: vec![song("s1", "Blue Monday")],
    });
    let frame = render(120, 40, &daemon, &mut client);
    assert!(
        frame.contains("Blur"),
        "matched artist must render;\n{frame}"
    );
    assert!(
        frame.contains("Low") && frame.contains("Heroes"),
        "both albums;\n{frame}"
    );
    assert!(
        frame.contains("Blue Monday"),
        "matched song must render;\n{frame}"
    );
    assert_eq!(
        frame.matches("David Bowie").count(),
        1,
        "two albums by one artist group under a single greyed label;\n{frame}"
    );
}

#[test]
fn search_album_result_shows_the_artist_name() {
    // Album-search rows show "artist - album"; the tree fallback omits the
    // artist, so the artist name appears only on the search path.
    let (daemon, mut client) = build_state();
    client.page = Page::Library;
    client.artists.filter_active = true;
    client.artists.filter = "blue".into();
    client.artists.search_results = Some(SearchResult3 {
        artist: vec![],
        album: vec![Album {
            id: "alb0".into(),
            name: "Kind of Blue".into(),
            artist: Some("Miles Davis".into()),
            artist_id: Some("a0".into()),
            cover_art: None,
            song_count: Some(9),
            original_release_date: None,
            duration: Some(2700),
            year: Some(1959),
            genre: None,
        }],
        song: vec![],
    });
    let frame = render(120, 30, &daemon, &mut client);
    assert!(
        frame.contains("Miles Davis"),
        "album search result must show the artist prefix, not the plain tree row;\n{}",
        frame
    );
}

#[test]
fn filter_active_with_empty_text_still_shows_the_search_title() {
    // searching = filter_active || !filter.is_empty(); pressing `/` activates
    // the filter before any text is typed, so the title must read Search.
    let (daemon, mut client) = build_state();
    client.page = Page::Library;
    client.artists.filter_active = true;
    client.artists.filter = String::new();
    let frame = render(120, 30, &daemon, &mut client);
    assert!(
        frame.contains("Search"),
        "an active-but-empty filter must show the Search title;\n{}",
        frame
    );
}

#[test]
fn search_artist_when_expanded_lists_their_albums() {
    // Regression #28: the search render dropped an expanded artist's albums, so
    // expanding a searched artist was a no-op with no drill-in to its music.
    let (mut daemon, mut client) = build_state();
    client.page = Page::Library;
    client.artists.filter_active = true;
    client.artists.filter = "cure".into();
    client.artists.search_results = Some(SearchResult3 {
        artist: vec![Artist {
            id: "a0".into(),
            name: "The Cure".into(),
            album_count: Some(1),
            cover_art: None,
        }],
        album: vec![],
        song: vec![],
    });
    client.artists.expanded.insert("a0".into());
    daemon.library.albums_cache.insert(
        "a0".into(),
        vec![Album {
            id: "alb0".into(),
            name: "Disintegration".into(),
            artist: Some("The Cure".into()),
            artist_id: Some("a0".into()),
            cover_art: None,
            song_count: Some(12),
            original_release_date: None,
            duration: Some(4000),
            year: Some(1989),
            genre: None,
        }],
    );
    let frame = render(120, 30, &daemon, &mut client);
    assert!(
        frame.contains("Disintegration"),
        "an expanded searched artist must list its albums;\n{}",
        frame
    );
}
