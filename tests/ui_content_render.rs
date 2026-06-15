//! Content + format invariants on the list renderers: duration math, multi-disc
//! track numbers, and per-row state colours (current / played / selected /
//! playing). Kills the equality, comparison, and arithmetic render mutants that
//! the style invariants alone leave alive.

mod common;

use common::{render_styled, song};
use ferrosonic::app::client_state::ClientState;
use ferrosonic::app::models::SongOption;
use ferrosonic::app::state::Page;
use ferrosonic::config::Config;
use ferrosonic::daemon::DaemonState;
use ferrosonic::subsonic::models::{Child, Playlist};

const W: u16 = 120;
const H: u16 = 30;

fn base() -> (DaemonState, ClientState) {
    let mut daemon = DaemonState::new(Config::new());
    daemon.config.theme = "default".into();
    (daemon, ClientState::default())
}

fn track_song(id: &str, title: &str, disc: Option<i32>, track: Option<i32>) -> Child {
    let mut s = song(id, title);
    s.disc_number = disc;
    s.track = track;
    s
}

#[test]
fn queue_colours_current_played_and_selected_rows_distinctly() {
    let (mut daemon, mut client) = base();
    client.page = Page::Queue;
    daemon.queue = (0..5)
        .map(|i| song(&format!("q{i}"), &format!("Track{i}")))
        .collect();
    daemon.queue_position = Some(2);
    client.queue_state.selected = Some(4);
    let c = *client.settings_state.theme_colors();
    let s = render_styled(W, H, &daemon, &mut client);

    let played = s.row_of("Track0").unwrap();
    let current = s.row_of("Track2").unwrap();
    let upcoming = s.row_of("Track3").unwrap();
    let selected = s.row_of("Track4").unwrap();

    assert!(
        s.row_has_fg(current, c.playing),
        "current track uses the playing colour"
    );
    assert!(
        s.rows_with("▶").contains(&current),
        "current track shows the play marker"
    );
    assert!(
        s.row_has_fg(played, c.played),
        "a track before the playhead is the played colour"
    );
    assert!(
        !s.row_has_fg(upcoming, c.played),
        "an upcoming track is not the played colour"
    );
    assert!(
        s.row_has_fg(selected, c.primary),
        "the selected upcoming track uses the primary colour"
    );
}

#[test]
fn playlist_duration_renders_minutes_and_seconds() {
    let (mut daemon, mut client) = base();
    client.page = Page::Playlists;
    daemon.library.playlists = vec![Playlist {
        id: "p".into(),
        name: "Mix".into(),
        owner: None,
        song_count: Some(3),
        duration: Some(605),
        cover_art: None,
        public: None,
        comment: None,
    }];
    let s = render_styled(W, H, &daemon, &mut client);
    // 605s = 10:05. `/`->`%` would give 5:05, `%`->`/` would give 10:10.
    assert!(
        s.text().contains("10:05"),
        "playlist duration must format as mm:ss (10:05 for 605s)\n{}",
        s.text()
    );
}

#[test]
fn library_song_pane_renders_multi_disc_track_numbers() {
    let (daemon, mut client) = base();
    client.page = Page::Library;
    client.artists.focus = 1;
    client.artists.songs = vec![
        track_song("d2", "DiscTwoSong", Some(2), Some(5)),
        track_song("d3", "DiscThreeSong", Some(3), Some(6)),
        track_song("dn", "NoDiscSong", None, Some(7)),
    ];
    client.artists.selected_song = Some(0);
    let s = render_styled(W, H, &daemon, &mut client);

    let d2 = s.row_of("DiscTwoSong").unwrap();
    let dn = s.row_of("NoDiscSong").unwrap();
    assert!(
        s.row_text(d2).contains("2.05"),
        "a disc-2 track must show disc.track when the album spans discs\n{}",
        s.text()
    );
    assert!(
        s.row_text(dn).contains("07"),
        "a no-disc track in a multi-disc album still shows its track number\n{}",
        s.text()
    );
}

#[test]
fn playlists_song_pane_colours_the_playing_song() {
    let (mut daemon, mut client) = base();
    client.page = Page::Playlists;
    client.playlists.focus = 1;
    client.playlists.songs = vec![song("x", "FirstSong"), song("y", "PlayingSong")];
    client.playlists.selected_song = Some(0); // selection on a different row
    daemon.queue = vec![song("y", "PlayingSong")];
    daemon.queue_position = Some(0);
    let c = *client.settings_state.theme_colors();
    let s = render_styled(W, H, &daemon, &mut client);

    let playing = s.row_of("PlayingSong").unwrap();
    assert!(
        s.row_has_fg_in(playing, 41, W, c.playing),
        "the playing song must use the playing colour in the song pane\n{}",
        s.text()
    );
}

#[test]
fn quickplay_song_pane_colours_the_playing_song() {
    let (mut daemon, mut client) = base();
    client.page = Page::QuickPlay;
    client.songs.focus = 1;
    client.songs.selected_option = Some(SongOption::Starred);
    daemon.library.starred_songs = vec![song("x", "FirstSong"), song("y", "PlayingSong")];
    client.songs.selected_index = Some(0);
    daemon.queue = vec![song("y", "PlayingSong")];
    daemon.queue_position = Some(0);
    let c = *client.settings_state.theme_colors();
    let s = render_styled(W, H, &daemon, &mut client);

    let playing = s.row_of("PlayingSong").unwrap();
    assert!(
        s.row_has_fg_in(playing, 23, W, c.playing),
        "the playing song must use the playing colour in the quickplay pane\n{}",
        s.text()
    );
}
