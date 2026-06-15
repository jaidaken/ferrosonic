//! Targeted kills for render mutants the colour invariants miss, using
//! discriminators that do not collide in the default theme (where playing ==
//! artist, primary == border_focused, song == album): the play glyph, the BOLD
//! modifier, the accent search border, and disc/track formatting.

mod common;

use common::{render_styled, song};
use ferrosonic::app::client_state::ClientState;
use ferrosonic::app::models::SongOption;
use ferrosonic::app::state::{FilterScope, Page};
use ferrosonic::config::Config;
use ferrosonic::daemon::DaemonState;
use ferrosonic::subsonic::models::{Album, Artist, Child, SearchResult3};
use ratatui::style::Modifier;

const W: u16 = 120;
const H: u16 = 30;

fn base() -> (DaemonState, ClientState) {
    let mut daemon = DaemonState::new(Config::new());
    daemon.config.theme = "default".into();
    (daemon, ClientState::default())
}

fn disc_song(id: &str, title: &str, disc: Option<i32>, track: Option<i32>) -> Child {
    let mut s = song(id, title);
    s.disc_number = disc;
    s.track = track;
    s
}

#[test]
fn playlists_play_glyph_marks_only_the_playing_song() {
    let (mut daemon, mut client) = base();
    client.page = Page::Playlists;
    client.playlists.focus = 1;
    client.playlists.songs = vec![song("x", "FirstSong"), song("y", "PlayingSong")];
    client.playlists.selected_song = Some(0);
    daemon.queue = vec![song("y", "PlayingSong")];
    daemon.queue_position = Some(0);
    let s = render_styled(W, H, &daemon, &mut client);
    let marked = s.rows_with("▶");
    let playing = s.row_of("PlayingSong").unwrap();
    let first = s.row_of("FirstSong").unwrap();
    assert!(
        marked.contains(&playing),
        "the playing song shows the glyph\n{}",
        s.text()
    );
    assert!(
        !marked.contains(&first),
        "a non-playing song does not\n{}",
        s.text()
    );
}

#[test]
fn quickplay_play_glyph_marks_only_the_playing_song() {
    let (mut daemon, mut client) = base();
    client.page = Page::QuickPlay;
    client.songs.focus = 1;
    client.songs.selected_option = Some(SongOption::Starred);
    daemon.library.starred_songs = vec![song("x", "FirstSong"), song("y", "PlayingSong")];
    client.songs.selected_index = Some(0);
    daemon.queue = vec![song("y", "PlayingSong")];
    daemon.queue_position = Some(0);
    let s = render_styled(W, H, &daemon, &mut client);
    let marked = s.rows_with("▶");
    let playing = s.row_of("PlayingSong").unwrap();
    let first = s.row_of("FirstSong").unwrap();
    assert!(
        marked.contains(&playing),
        "the playing song shows the glyph\n{}",
        s.text()
    );
    assert!(
        !marked.contains(&first),
        "a non-playing song does not\n{}",
        s.text()
    );
}

#[test]
fn queue_bolds_only_the_selected_upcoming_row() {
    let (mut daemon, mut client) = base();
    client.page = Page::Queue;
    daemon.queue = (0..5)
        .map(|i| song(&format!("q{i}"), &format!("Track{i}")))
        .collect();
    daemon.queue_position = Some(0); // Track0 current; Track1..4 upcoming
    client.queue_state.selected = Some(2);
    let s = render_styled(W, H, &daemon, &mut client);
    let selected = s.row_of("Track2").unwrap();
    let other = s.row_of("Track3").unwrap();
    assert!(
        s.row_has_modifier_in(selected, 1, W, Modifier::BOLD),
        "the selected upcoming row is bold\n{}",
        s.text()
    );
    assert!(
        !s.row_has_modifier_in(other, 1, W, Modifier::BOLD),
        "a non-selected upcoming row is not bold\n{}",
        s.text()
    );
}

#[test]
fn library_tree_bolds_only_the_selected_row() {
    let (mut daemon, mut client) = base();
    client.page = Page::Library;
    client.artists.focus = 0;
    daemon.library.artists = vec![
        Artist {
            id: "a0".into(),
            name: "AlphaArtist".into(),
            album_count: Some(1),
            cover_art: None,
        },
        Artist {
            id: "a1".into(),
            name: "BetaArtist".into(),
            album_count: Some(1),
            cover_art: None,
        },
    ];
    client.artists.selected_index = Some(0);
    let s = render_styled(W, H, &daemon, &mut client);
    let sel = s.row_of("AlphaArtist").unwrap();
    let other = s.row_of("BetaArtist").unwrap();
    assert!(
        s.row_has_modifier_in(sel, 1, 40, Modifier::BOLD),
        "selected tree row is bold\n{}",
        s.text()
    );
    assert!(
        !s.row_has_modifier_in(other, 1, 40, Modifier::BOLD),
        "non-selected tree row is not bold\n{}",
        s.text()
    );
}

#[test]
fn library_search_border_uses_accent_when_filter_active() {
    let (daemon, mut client) = base();
    client.page = Page::Library;
    client.artists.focus = 0;
    client.artists.filter_active = true;
    client.artists.filter = String::new();
    let accent = client.settings_state.theme_colors().accent;
    let s = render_styled(W, H, &daemon, &mut client);
    // searching colours the tree's left border (col 0, content rows) accent;
    // `||`->`&&` drops it. Rows 1..6 stay in the tree, clear of header/footer.
    assert!(
        (1..6).any(|y| s.cell(0, y).fg == accent),
        "an active filter must colour the tree border with accent\n{}",
        s.text()
    );
}

#[test]
fn library_single_disc_album_omits_the_disc_prefix() {
    let (daemon, mut client) = base();
    client.page = Page::Library;
    client.artists.focus = 1;
    client.artists.songs = vec![
        disc_song("s1", "SongOne", Some(1), Some(1)),
        disc_song("s2", "SongTwo", Some(1), Some(2)),
    ];
    let s = render_styled(W, H, &daemon, &mut client);
    let row = s.row_of("SongOne").unwrap();
    // has_multiple_discs = any d > 1; all disc 1 -> false -> "01. " not "1.01.".
    // `>`->`>=` would make every disc-1 album look multi-disc.
    assert!(
        s.row_text(row).contains("01"),
        "single-disc track shows plain number\n{}",
        s.text()
    );
    assert!(
        !s.row_text(row).contains("1.01"),
        "single-disc track has no disc prefix\n{}",
        s.text()
    );
}

#[test]
fn quickplay_selected_option_uses_highlight_colour() {
    let (mut daemon, mut client) = base();
    client.page = Page::QuickPlay;
    client.songs.focus = 0;
    client.songs.selected_option = Some(SongOption::Random);
    daemon.library.random_songs = vec![song("r", "PlaceholderTune")];
    let hl = client.settings_state.theme_colors().highlight_fg;
    let s = render_styled(W, H, &daemon, &mut client);
    let selected = s.row_of("Random").unwrap();
    let other = s.row_of("Starred").unwrap();
    assert!(
        s.row_has_fg_in(selected, 1, 22, hl),
        "selected option uses highlight_fg\n{}",
        s.text()
    );
    assert!(
        !s.row_has_fg_in(other, 1, 22, hl),
        "a non-selected option does not use highlight_fg\n{}",
        s.text()
    );
}

#[test]
fn library_tree_album_omits_artist_prefix_with_stale_search_results() {
    // A tree album (filter empty) keeps the plain format even if search_results
    // linger and scope is Albums. Kills the `!filter.is_empty() && ...` -> `||`.
    let (mut daemon, mut client) = base();
    client.page = Page::Library;
    daemon.library.artists = vec![Artist {
        id: "a0".into(),
        name: "TheArtist".into(),
        album_count: Some(1),
        cover_art: None,
    }];
    daemon.library.albums_cache.insert(
        "a0".into(),
        vec![Album {
            id: "alb".into(),
            name: "TheAlbum".into(),
            artist: Some("ZZArtistPrefix".into()),
            artist_id: Some("a0".into()),
            cover_art: None,
            song_count: Some(1),
            original_release_date: None,
            duration: Some(100),
            year: Some(2000),
            genre: None,
        }],
    );
    client.artists.expanded.insert("a0".into());
    client.artists.filter = String::new();
    client.artists.filter_scope = FilterScope::Albums;
    client.artists.search_results = Some(SearchResult3 {
        artist: vec![],
        album: vec![],
        song: vec![],
    });
    let s = render_styled(W, H, &daemon, &mut client);
    assert!(
        s.text().contains("TheAlbum"),
        "the tree album must render\n{}",
        s.text()
    );
    assert!(
        !s.text().contains("ZZArtistPrefix"),
        "a tree album must not show the artist prefix (that is the search format)\n{}",
        s.text()
    );
}
