//! Indicator invariants through the shared song-line styler: the playing track
//! shows the ▶ marker and starred songs show ★, on the correct row and not on
//! others. Asserts are per-row (the now-playing bar also shows the current
//! track, so whole-screen counts would be brittle).

mod common;

use common::{render_styled, song, song_starred};
use ferrosonic::app::client_state::ClientState;
use ferrosonic::app::state::Page;
use ferrosonic::config::Config;
use ferrosonic::daemon::DaemonState;

fn base() -> (DaemonState, ClientState) {
    let mut daemon = DaemonState::new(Config::new());
    daemon.config.theme = "default".into();
    (daemon, ClientState::default())
}

#[test]
fn queue_marks_the_playing_track_and_only_that_track() {
    let (mut daemon, mut client) = base();
    client.page = Page::Queue;
    daemon.queue = vec![
        song("q0", "Track Zero"),
        song("q1", "Track One"),
        song("q2", "Track Two"),
    ];
    daemon.queue_position = Some(1);

    let screen = render_styled(100, 24, &daemon, &mut client);
    let marked = screen.rows_with("▶");

    assert!(
        marked
            .iter()
            .any(|&y| screen.row_text(y).contains("Track One")),
        "the playing track must show the ▶ marker;\n{}",
        screen.text()
    );
    assert!(
        !marked
            .iter()
            .any(|&y| screen.row_text(y).contains("Track Zero")),
        "a queued, non-playing track must not show ▶;\n{}",
        screen.text()
    );
    assert!(
        !marked
            .iter()
            .any(|&y| screen.row_text(y).contains("Track Two")),
        "a queued, non-playing track must not show ▶;\n{}",
        screen.text()
    );
}

#[test]
fn library_song_pane_stars_the_starred_song_only() {
    let (daemon, mut client) = base();
    client.page = Page::Library;
    client.artists.songs = vec![song_starred("s0", "Starred Song"), song("s1", "Plain Song")];
    client.artists.focus = 1;

    let screen = render_styled(100, 24, &daemon, &mut client);
    let starred = screen.rows_with("★");

    assert!(
        starred
            .iter()
            .any(|&y| screen.row_text(y).contains("Starred Song")),
        "the starred song must show ★;\n{}",
        screen.text()
    );
    assert!(
        !starred
            .iter()
            .any(|&y| screen.row_text(y).contains("Plain Song")),
        "an unstarred song must not show ★;\n{}",
        screen.text()
    );
}

#[test]
fn library_song_pane_marks_the_playing_song_only() {
    let (mut daemon, mut client) = base();
    client.page = Page::Library;
    let playing = song("x", "Now Playing");
    client.artists.songs = vec![playing.clone(), song("y", "Other Song")];
    client.artists.focus = 1;
    daemon.queue = vec![playing];
    daemon.queue_position = Some(0);

    let screen = render_styled(100, 24, &daemon, &mut client);
    let marked = screen.rows_with("▶");

    assert!(
        marked
            .iter()
            .any(|&y| screen.row_text(y).contains("Now Playing")),
        "the playing song must show ▶ in the list;\n{}",
        screen.text()
    );
    assert!(
        !marked
            .iter()
            .any(|&y| screen.row_text(y).contains("Other Song")),
        "a non-playing song must not show ▶;\n{}",
        screen.text()
    );
}
