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
    // One search returns all kinds: a matched artist, name-matched albums under
    // one greyed parent-artist label, and a title-matched song nested below.
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
    let mut bowie_song = song("s1", "Blue Jean");
    bowie_song.artist = Some("New Order".into());
    bowie_song.album = Some("Power".into());
    client.artists.search_results = Some(SearchResult3 {
        artist: vec![Artist {
            id: "a-blur".into(),
            name: "Blur".into(),
            album_count: Some(2),
            cover_art: None,
        }],
        album: vec![alb("alb1", "Blackstar"), alb("alb2", "Blackout")],
        song: vec![bowie_song],
    });
    let frame = render(120, 40, &daemon, &mut client);
    assert!(
        frame.contains("Blur"),
        "matched artist must render;\n{frame}"
    );
    assert!(
        frame.contains("Blackstar") && frame.contains("Blackout"),
        "both name-matched albums;\n{frame}"
    );
    assert!(
        frame.contains("Blue Jean"),
        "title-matched song must render;\n{frame}"
    );
    assert_eq!(
        frame.matches("David Bowie").count(),
        1,
        "two albums by one artist group under a single greyed label;\n{frame}"
    );
}

#[test]
fn search_matched_artist_nests_its_album_without_a_duplicate_label() {
    // Artist matches AND has a name-matching album: the album nests under the
    // matched artist; no separate greyed header repeats the same artist.
    let (daemon, mut client) = build_state();
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
        album: vec![Album {
            id: "alb0".into(),
            name: "Curefest".into(),
            artist: Some("The Cure".into()),
            artist_id: Some("a0".into()),
            cover_art: None,
            song_count: Some(12),
            original_release_date: None,
            duration: Some(4000),
            year: Some(1989),
            genre: None,
        }],
        song: vec![],
    });
    let frame = render(120, 30, &daemon, &mut client);
    assert!(
        frame.contains("Curefest"),
        "matched album must nest under the artist;\n{frame}"
    );
    assert_eq!(
        frame.matches("The Cure").count(),
        1,
        "matched artist appears once, not duplicated as a greyed header;\n{frame}"
    );
}

#[test]
fn search_album_match_does_not_show_its_songs_in_the_tree() {
    // A name-matched album is a leaf: its tracks load into the song pane on
    // select, they do not stretch the tree. Only title-matched songs nest.
    let (daemon, mut client) = build_state();
    client.page = Page::Library;
    client.artists.filter_active = true;
    client.artists.filter = "graceland".into();
    let mut off_title = song("s0", "Diamonds on the Soles");
    off_title.album = Some("Graceland".into());
    client.artists.search_results = Some(SearchResult3 {
        artist: vec![],
        album: vec![Album {
            id: "alb0".into(),
            name: "Graceland".into(),
            artist: Some("Paul Simon".into()),
            artist_id: Some("a0".into()),
            cover_art: None,
            song_count: Some(11),
            original_release_date: None,
            duration: Some(3000),
            year: Some(1986),
            genre: None,
        }],
        song: vec![off_title],
    });
    let frame = render(120, 30, &daemon, &mut client);
    assert!(
        frame.contains("Graceland"),
        "album match must render;\n{frame}"
    );
    assert!(
        !frame.contains("Diamonds"),
        "an album match must not pull in its tracks (title did not match);\n{frame}"
    );
}

#[test]
fn search_expanded_song_artist_nests_the_matched_song_under_its_catalog_album() {
    // A title match exposes its artist (greyed, via the song's artistId);
    // expanding it shows the catalogue with the matched song still nested.
    let (mut daemon, mut client) = build_state();
    client.page = Page::Library;
    client.artists.filter_active = true;
    client.artists.filter = "redempt".into();
    let mut s = song("s0", "Redemption Song");
    s.artist = Some("Bob Marley".into());
    s.artist_id = Some("a-marley".into());
    s.album = Some("Uprising".into());
    client.artists.search_results = Some(SearchResult3 {
        artist: vec![],
        album: vec![],
        song: vec![s],
    });
    client.artists.expanded.insert("a-marley".into());
    let cat = |id: &str, name: &str, year: i32| Album {
        id: id.into(),
        name: name.into(),
        artist: Some("Bob Marley".into()),
        artist_id: Some("a-marley".into()),
        cover_art: None,
        song_count: Some(10),
        original_release_date: None,
        duration: Some(2400),
        year: Some(year),
        genre: None,
    };
    daemon.library.albums_cache.insert(
        "a-marley".into(),
        vec![
            cat("alb-up", "Uprising", 1980),
            cat("alb-ex", "Exodus", 1977),
        ],
    );
    let frame = render(120, 40, &daemon, &mut client);
    assert!(
        frame.contains("Exodus") && frame.contains("Uprising"),
        "the expanded artist shows its whole catalogue;\n{frame}"
    );
    let up = frame.find("Uprising");
    let song_at = frame.find("Redemption Song");
    assert!(
        matches!((up, song_at), (Some(u), Some(s)) if s > u),
        "the matched song nests under its album, after it;\n{frame}"
    );
}

#[test]
fn search_matched_song_nests_under_greyed_album_and_artist() {
    // Title match: the song is the selectable leaf; its album and artist render
    // as greyed context above it.
    let (daemon, mut client) = build_state();
    client.page = Page::Library;
    client.artists.filter_active = true;
    client.artists.filter = "redemption".into();
    let mut s = song("s0", "Redemption Song");
    s.artist = Some("Bob Marley".into());
    s.album = Some("Uprising".into());
    client.artists.search_results = Some(SearchResult3 {
        artist: vec![],
        album: vec![],
        song: vec![s],
    });
    let frame = render(120, 30, &daemon, &mut client);
    assert!(
        frame.contains("Bob Marley") && frame.contains("Uprising"),
        "artist and album render as context for the matched song;\n{frame}"
    );
    assert!(
        frame.contains("Redemption Song"),
        "the matched song renders;\n{frame}"
    );
}

#[test]
fn search_collapsed_artist_does_not_dump_full_catalog() {
    // A collapsed matched artist shows only matched albums (none here); its
    // cached full catalog must not leak in.
    let (mut daemon, mut client) = build_state();
    client.page = Page::Library;
    client.artists.filter_active = true;
    client.artists.filter = "cure".into();
    client.artists.search_results = Some(SearchResult3 {
        artist: vec![Artist {
            id: "a0".into(),
            name: "The Cure".into(),
            album_count: Some(2),
            cover_art: None,
        }],
        album: vec![],
        song: vec![],
    });
    daemon.library.albums_cache.insert(
        "a0".into(),
        vec![Album {
            id: "x".into(),
            name: "Faith".into(),
            artist: Some("The Cure".into()),
            artist_id: Some("a0".into()),
            cover_art: None,
            song_count: Some(8),
            original_release_date: None,
            duration: Some(2000),
            year: Some(1981),
            genre: None,
        }],
    );
    let frame = render(120, 30, &daemon, &mut client);
    assert!(frame.contains("The Cure"), "artist must render;\n{frame}");
    assert!(
        !frame.contains("Faith"),
        "a collapsed search artist must not dump its cached catalog;\n{frame}"
    );
}

#[test]
fn search_songs_match_title_only_not_artist_name() {
    // search3 also matches a song by its artist, so a "beach" query returns
    // every Beach House track. Only titles containing the query may render.
    let (daemon, mut client) = build_state();
    client.page = Page::Library;
    client.artists.filter_active = true;
    client.artists.filter = "beach".into();
    let mut artist_match = song("s0", "Bluebird");
    artist_match.artist = Some("Beach House".into());
    let mut title_match = song("s1", "Beachball");
    title_match.artist = Some("R.E.M.".into());
    client.artists.search_results = Some(SearchResult3 {
        artist: vec![],
        album: vec![],
        song: vec![artist_match, title_match],
    });
    let frame = render(120, 30, &daemon, &mut client);
    assert!(
        frame.contains("Beachball"),
        "a song whose title contains the query must render;\n{frame}"
    );
    assert!(
        !frame.contains("Bluebird"),
        "a song matched only by its artist name must not render;\n{frame}"
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
