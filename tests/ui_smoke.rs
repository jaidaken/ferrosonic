//! UI page snapshot tests via ratatui TestBackend.

mod common;

use common::{render, song};
use ferrosonic::app::client_state::ClientState;
use ferrosonic::app::state::Page;
use ferrosonic::config::Config;
use ferrosonic::daemon::DaemonState;

fn build_state() -> (DaemonState, ClientState) {
    let config = Config::new();
    let mut daemon = DaemonState::new(config);
    daemon.config.theme = "default".into();
    let client = ClientState::default();
    (daemon, client)
}

#[test]
fn library_page_renders_without_panic() {
    let (daemon, mut client) = build_state();
    client.page = Page::Library;
    let frame = render(80, 24, &daemon, &mut client);
    insta::assert_snapshot!(frame);
}

#[test]
fn queue_page_renders_without_panic() {
    let (daemon, mut client) = build_state();
    client.page = Page::Queue;
    let frame = render(80, 24, &daemon, &mut client);
    insta::assert_snapshot!(frame);
}

#[test]
fn quickplay_page_renders_without_panic() {
    let (daemon, mut client) = build_state();
    client.page = Page::QuickPlay;
    let frame = render(80, 24, &daemon, &mut client);
    insta::assert_snapshot!(frame);
}

#[test]
fn playlists_page_renders_without_panic() {
    let (daemon, mut client) = build_state();
    client.page = Page::Playlists;
    let frame = render(80, 24, &daemon, &mut client);
    insta::assert_snapshot!(frame);
}

#[test]
fn server_page_renders_without_panic() {
    let (daemon, mut client) = build_state();
    client.page = Page::Server;
    let frame = render(80, 24, &daemon, &mut client);
    insta::assert_snapshot!(frame);
}

#[test]
fn settings_page_renders_without_panic() {
    let (daemon, mut client) = build_state();
    client.page = Page::Settings;
    let frame = render(80, 24, &daemon, &mut client);
    insta::assert_snapshot!(frame);
}

#[test]
fn now_playing_widget_renders_with_a_loaded_track() {
    let (mut daemon, mut client) = build_state();
    daemon.queue.push(song("a", "A Track"));
    daemon.queue_position = Some(0);
    daemon.now_playing.song = Some(song("a", "A Track"));
    daemon.now_playing.state = ferrosonic::daemon::state::PlaybackState::Playing;
    daemon.now_playing.position = 30.0;
    daemon.now_playing.duration = 180.0;
    let frame = render(80, 24, &daemon, &mut client);
    assert!(
        frame.contains("A Track"),
        "track title must appear; got:\n{}",
        frame
    );
    insta::assert_snapshot!(frame);
}
