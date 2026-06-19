//! ui/pages/queue.rs: render with current/played/starred/track-info combinations.

mod common;

use common::render;
use ferrosonic::app::client_state::ClientState;
use ferrosonic::app::state::Page;
use ferrosonic::config::Config;
use ferrosonic::daemon::DaemonState;
use ferrosonic::subsonic::models::Child;

fn song(id: &str, title: &str) -> Child {
    Child {
        id: id.into(),
        title: title.into(),
        parent: None,
        is_dir: false,
        album: Some("Album".into()),
        artist: Some("Artist".into()),
        artist_id: None,
        track: None,
        year: None,
        genre: None,
        cover_art: None,
        size: None,
        content_type: None,
        suffix: None,
        duration: Some(180),
        bit_rate: None,
        path: None,
        disc_number: None,
        starred: None,
    }
}

fn build_state() -> (DaemonState, ClientState) {
    let config = Config::new();
    let mut daemon = DaemonState::new(config);
    daemon.config.theme = "default".into();
    let client = ClientState {
        page: Page::Queue,
        ..ClientState::default()
    };
    (daemon, client)
}

#[test]
fn queue_with_current_track_renders_play_indicator() {
    let (mut daemon, mut client) = build_state();
    daemon.queue = vec![song("a", "A"), song("b", "B"), song("c", "C")];
    daemon.queue_position = Some(1);
    let frame = render(80, 24, &daemon, &mut client);
    insta::assert_snapshot!(frame);
}

#[test]
fn queue_with_played_tracks_uses_played_style() {
    let (mut daemon, mut client) = build_state();
    daemon.queue = vec![song("a", "A"), song("b", "B"), song("c", "C")];
    daemon.queue_position = Some(2);
    let frame = render(80, 24, &daemon, &mut client);
    insta::assert_snapshot!(frame);
}

#[test]
fn queue_with_starred_song_renders_star_indicator() {
    let (mut daemon, mut client) = build_state();
    let mut s = song("a", "A");
    s.starred = Some("2024".into());
    daemon.queue = vec![s];
    let frame = render(80, 24, &daemon, &mut client);
    insta::assert_snapshot!(frame);
}

#[test]
fn queue_with_track_number_only_renders_hash_track() {
    let (mut daemon, mut client) = build_state();
    let mut s = song("a", "A");
    s.track = Some(7);
    daemon.queue = vec![s];
    let frame = render(80, 24, &daemon, &mut client);
    insta::assert_snapshot!(frame);
}

#[test]
fn queue_with_disc_and_track_renders_disc_track_format() {
    let (mut daemon, mut client) = build_state();
    let mut s = song("a", "A");
    s.track = Some(3);
    s.disc_number = Some(2);
    daemon.queue = vec![s];
    let frame = render(80, 24, &daemon, &mut client);
    insta::assert_snapshot!(frame);
}

#[test]
fn queue_with_disc_one_and_track_uses_hash_format() {
    let (mut daemon, mut client) = build_state();
    let mut s = song("a", "A");
    s.track = Some(3);
    s.disc_number = Some(1);
    daemon.queue = vec![s];
    let frame = render(80, 24, &daemon, &mut client);
    insta::assert_snapshot!(frame);
}

#[test]
fn queue_with_selected_played_uses_bold_played_style() {
    let (mut daemon, mut client) = build_state();
    daemon.queue = vec![song("a", "A"), song("b", "B"), song("c", "C")];
    daemon.queue_position = Some(2);
    client.queue_state.selected = Some(0);
    let frame = render(80, 24, &daemon, &mut client);
    insta::assert_snapshot!(frame);
}

#[test]
fn queue_with_selected_upcoming_uses_primary_style() {
    let (mut daemon, mut client) = build_state();
    daemon.queue = vec![song("a", "A"), song("b", "B"), song("c", "C")];
    daemon.queue_position = Some(0);
    client.queue_state.selected = Some(2);
    let frame = render(80, 24, &daemon, &mut client);
    insta::assert_snapshot!(frame);
}

#[test]
fn queue_with_no_position_renders_all_upcoming() {
    let (mut daemon, mut client) = build_state();
    daemon.queue = vec![song("a", "A"), song("b", "B")];
    let frame = render(80, 24, &daemon, &mut client);
    insta::assert_snapshot!(frame);
}

#[test]
fn empty_queue_renders_placeholder_or_empty() {
    let (daemon, mut client) = build_state();
    let frame = render(80, 24, &daemon, &mut client);
    insta::assert_snapshot!(frame);
}
