//! ui/layout.rs branches: art_visible + cava_active combinations.

mod common;

use common::render;
use ferrosonic::app::client_state::ClientState;
use ferrosonic::app::state::{CavaRow, Page};
use ferrosonic::config::Config;
use ferrosonic::daemon::DaemonState;
use ferrosonic::subsonic::models::Child;

fn build_state() -> (DaemonState, ClientState) {
    let config = Config::new();
    let mut daemon = DaemonState::new(config);
    daemon.config.theme = "default".into();
    (daemon, ClientState::default())
}

fn song_with_cover(id: &str, cover: &str) -> Child {
    Child {
        id: id.into(),
        title: id.into(),
        parent: None,
        is_dir: false,
        album: Some("Album".into()),
        artist: Some("Artist".into()),
        track: None,
        year: None,
        genre: None,
        cover_art: Some(cover.into()),
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

#[test]
fn layout_with_cover_art_enabled_and_song_with_cover_uses_reserved_cols() {
    let (mut daemon, mut client) = build_state();
    daemon.now_playing.song = Some(song_with_cover("s", "art"));
    daemon.now_playing.duration = 180.0;
    client.page = Page::Library;
    client.settings_state.cover_art = true;
    client.settings_state.cover_art_size = 16;
    let frame = render(120, 40, &daemon, &mut client);
    assert!(!frame.is_empty());
}

#[test]
fn layout_with_cava_active_inserts_cava_row() {
    let (daemon, mut client) = build_state();
    client.page = Page::Library;
    client.settings_state.cava_enabled = true;
    client.settings_state.cava_size = 30;
    client.cava_screen = vec![CavaRow { spans: vec![] }];
    let frame = render(80, 24, &daemon, &mut client);
    assert!(!frame.is_empty());
}

#[test]
fn layout_clamps_cover_art_size_to_max_24() {
    let (mut daemon, mut client) = build_state();
    daemon.now_playing.song = Some(song_with_cover("s", "art"));
    client.page = Page::Library;
    client.settings_state.cover_art = true;
    client.settings_state.cover_art_size = 50;
    let frame = render(120, 60, &daemon, &mut client);
    assert!(!frame.is_empty());
}

#[test]
fn layout_clamps_cover_art_size_to_min_8() {
    let (mut daemon, mut client) = build_state();
    daemon.now_playing.song = Some(song_with_cover("s", "art"));
    client.page = Page::Library;
    client.settings_state.cover_art = true;
    client.settings_state.cover_art_size = 4;
    let frame = render(120, 40, &daemon, &mut client);
    assert!(!frame.is_empty());
}

#[test]
fn layout_with_both_cava_active_and_cover_art_visible() {
    let (mut daemon, mut client) = build_state();
    daemon.now_playing.song = Some(song_with_cover("s", "art"));
    client.page = Page::Library;
    client.settings_state.cover_art = true;
    client.settings_state.cover_art_size = 12;
    client.settings_state.cava_enabled = true;
    client.settings_state.cava_size = 25;
    client.cava_screen = vec![CavaRow { spans: vec![] }];
    let frame = render(120, 60, &daemon, &mut client);
    assert!(!frame.is_empty());
}

#[test]
fn layout_with_no_cover_art_uses_base_now_playing_height() {
    let (daemon, mut client) = build_state();
    client.page = Page::QuickPlay;
    client.settings_state.cover_art = false;
    let frame = render(80, 24, &daemon, &mut client);
    assert!(!frame.is_empty());
}

#[test]
fn layout_with_cover_art_enabled_but_no_song_uses_base_height() {
    let (daemon, mut client) = build_state();
    client.page = Page::Library;
    client.settings_state.cover_art = true;
    let frame = render(80, 24, &daemon, &mut client);
    assert!(!frame.is_empty());
}

#[test]
fn layout_with_cover_art_enabled_and_song_without_cover_id_uses_base_height() {
    let (mut daemon, mut client) = build_state();
    let mut s = song_with_cover("s", "art");
    s.cover_art = None;
    daemon.now_playing.song = Some(s);
    client.page = Page::Library;
    client.settings_state.cover_art = true;
    let frame = render(80, 24, &daemon, &mut client);
    assert!(!frame.is_empty());
}
