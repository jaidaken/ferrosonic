//! UI render tests with populated state: artists, queue items, cava data.

mod common;

use common::{render, song};
use ferrosonic::app::client_state::ClientState;
use ferrosonic::app::state::{CavaColor, CavaRow, CavaSpan, Page};
use ferrosonic::daemon::state::PlaybackState;
use ferrosonic::config::Config;
use ferrosonic::daemon::DaemonState;
use ferrosonic::subsonic::models::{Album, Artist};

fn build_state() -> (DaemonState, ClientState) {
    let config = Config::new();
    let mut daemon = DaemonState::new(config);
    daemon.config.theme = "default".into();
    let client = ClientState::default();
    (daemon, client)
}

fn artist(id: &str, name: &str) -> Artist {
    Artist {
        id: id.into(),
        name: name.into(),
        album_count: Some(2),
        cover_art: None,
    }
}

fn album(id: &str, name: &str) -> Album {
    Album {
        id: id.into(),
        name: name.into(),
        artist: Some("Test Artist".into()),
        artist_id: Some("artist-0".into()),
        cover_art: None,
        song_count: Some(10),
        original_release_date: None,
        duration: Some(2400),
        year: Some(2020),
        genre: None,
    }
}

#[test]
fn library_page_with_artists_renders_their_names() {
    let (mut daemon, mut client) = build_state();
    daemon.library.artists = vec![artist("a0", "The Cure"), artist("a1", "Pixies")];
    client.page = Page::Library;
    let frame = render(80, 24, &daemon, &mut client);
    assert!(frame.contains("Cure"), "expected Cure in frame:\n{}", frame);
    assert!(frame.contains("Pixies"));
}

#[test]
fn library_page_with_expanded_artist_renders_albums() {
    let (mut daemon, mut client) = build_state();
    daemon.library.artists = vec![artist("a0", "Joy Division")];
    daemon.library.albums_cache.insert(
        "a0".into(),
        vec![album("alb0", "Closer"), album("alb1", "Unknown Pleasures")],
    );
    client.page = Page::Library;
    client.artists.expanded.insert("a0".into());
    client.artists.selected_index = Some(0);
    let frame = render(80, 24, &daemon, &mut client);
    assert!(frame.contains("Joy Division"));
    assert!(frame.contains("Closer") || frame.contains("Unknown"));
}

#[test]
fn queue_page_with_songs_renders_titles() {
    let (mut daemon, mut client) = build_state();
    daemon.queue = vec![song("s0", "Pictures of You"), song("s1", "Lovesong")];
    daemon.queue_position = Some(0);
    daemon.now_playing.song = Some(daemon.queue[0].clone());
    daemon.now_playing.state = PlaybackState::Playing;
    client.page = Page::Queue;
    let frame = render(80, 24, &daemon, &mut client);
    assert!(
        frame.contains("Pictures of You") || frame.contains("Lovesong"),
        "expected queue titles in frame:\n{}",
        frame
    );
}

#[test]
fn settings_page_renders_all_sections() {
    let (daemon, mut client) = build_state();
    client.page = Page::Settings;
    let frame = render(120, 40, &daemon, &mut client);
    // Section headings live in the settings page renderer.
    for expected in [
        "Display",
        "Now Playing",
        "Playback",
        "System",
        "Theme",
        "Cava",
        "Cover Art",
        "Repeat",
        "Daemon",
    ] {
        assert!(
            frame.contains(expected),
            "settings frame should contain {:?}\n{}",
            expected,
            frame
        );
    }
}

#[test]
fn cava_band_renders_when_enabled_with_data() {
    let (mut daemon, mut client) = build_state();
    daemon.config.cava = true;
    client.settings_state.cava_enabled = true;
    client.settings_state.cava_size = 40;
    client.cava_screen = vec![
        CavaRow {
            spans: vec![CavaSpan {
                text: "x".repeat(80),
                fg: CavaColor::Indexed(2),
                bg: CavaColor::Default,
            }],
        };
        4
    ];
    client.page = Page::Library;
    let frame = render(80, 30, &daemon, &mut client);
    assert!(frame.contains('x'), "cava row content must appear in frame");
}

#[test]
fn now_playing_widget_renders_quality_row_when_set() {
    let (mut daemon, mut client) = build_state();
    daemon.queue.push(song("a", "Track"));
    daemon.queue_position = Some(0);
    daemon.now_playing.song = Some(song("a", "Track"));
    daemon.now_playing.state = PlaybackState::Playing;
    daemon.now_playing.position = 12.0;
    daemon.now_playing.duration = 240.0;
    daemon.now_playing.sample_rate = Some(44100);
    daemon.now_playing.bit_depth = Some(16);
    daemon.now_playing.format = Some("FLAC".into());
    daemon.now_playing.channels = Some("Stereo".into());
    let frame = render(80, 24, &daemon, &mut client);
    assert!(frame.contains("44") || frame.contains("kHz"));
    assert!(frame.contains("FLAC") || frame.contains("flac"));
}

#[test]
fn library_page_with_song_pane_renders_styled_lines() {
    let (mut daemon, mut client) = build_state();
    daemon.library.artists = vec![artist("a0", "Test Artist")];
    daemon
        .library
        .albums_cache
        .insert("a0".into(), vec![album("alb0", "Test Album")]);
    client.page = Page::Library;
    client.artists.songs = vec![song("s0", "First Song"), song("s1", "Second Song")];
    client.artists.selected_song = Some(0);
    client.artists.focus = 1;
    let frame = render(100, 30, &daemon, &mut client);
    assert!(
        frame.contains("First Song") || frame.contains("Second Song"),
        "song pane should render titles via styled_lines:\n{}",
        frame
    );
}

#[test]
fn quickplay_with_random_songs_renders_titles() {
    use ferrosonic::app::models::SongOption;
    let (mut daemon, mut client) = build_state();
    daemon.library.random_songs = vec![
        song("r0", "Random Track One"),
        song("r1", "Random Track Two"),
    ];
    client.page = Page::QuickPlay;
    client.songs.selected_option = Some(SongOption::Random);
    let frame = render(100, 30, &daemon, &mut client);
    assert!(
        frame.contains("Random Track One") || frame.contains("Random Track Two"),
        "quickplay should render random song titles via styled_lines:\n{}",
        frame
    );
}

#[test]
fn footer_with_notification_renders_message() {
    let (daemon, mut client) = build_state();
    client.notify("hello world");
    client.page = Page::Library;
    let frame = render(80, 24, &daemon, &mut client);
    assert!(
        frame.contains("hello world"),
        "notification must render in footer; got:\n{}",
        frame
    );
}
