//! Responsive header, pane, and media-band rendering regressions.

mod common;

use common::{render, song};
use ferrosonic::app::client_state::ClientState;
use ferrosonic::app::models::SongOption;
use ferrosonic::app::state::Page;
use ferrosonic::config::Config;
use ferrosonic::daemon::DaemonState;
use ferrosonic::subsonic::models::{Artist, Playlist};
use ferrosonic::ui::header::{Header, HeaderRegion};
use ratatui::layout::Rect;

fn build_state() -> (DaemonState, ClientState) {
    let mut daemon = DaemonState::new(Config::new());
    daemon.config.theme = "default".into();
    (daemon, ClientState::default())
}

fn populate(daemon: &mut DaemonState) {
    daemon.library.artists = vec![Artist {
        id: "artist-1".into(),
        name: "Vertical Artist".into(),
        album_count: Some(1),
        cover_art: None,
    }];
    daemon.library.starred_songs = vec![song("song-1", "A Song With A Readable Title")];
    daemon.library.playlists = vec![Playlist {
        id: "playlist-1".into(),
        name: "Vertical Playlist".into(),
        song_count: Some(1),
        duration: Some(180),
        owner: None,
        public: None,
        cover_art: None,
        comment: None,
    }];
}

#[test]
fn wrapped_header_keeps_all_tabs_and_controls_reachable() {
    let area = Rect::new(0, 0, 60, Header::required_height(60));
    assert_eq!(area.height, 2);
    assert_eq!(
        Header::region_at(area, 13, 1),
        Some(HeaderRegion::Tab(Page::Settings))
    );
    assert_eq!(
        Header::region_at(area, 41, 1),
        Some(HeaderRegion::PrevButton)
    );

    let (mut daemon, mut client) = build_state();
    populate(&mut daemon);
    let frame = render(60, 50, &daemon, &mut client);
    for label in [
        "F1 Library",
        "F2 Queue",
        "F3 Quick Play",
        "F4 Playlists",
        "F5 Server",
        "F6 Settings",
    ] {
        assert!(
            frame.contains(label),
            "missing header tab {label:?}\n{frame}"
        );
    }
}

#[test]
fn library_and_playlists_stack_below_the_width_threshold() {
    for page in [Page::Library, Page::Playlists] {
        let (mut daemon, mut client) = build_state();
        populate(&mut daemon);
        client.page = page;
        let frame = render(60, 50, &daemon, &mut client);
        let left = client.layout.content_left.expect("first pane");
        let right = client.layout.content_right.expect("second pane");
        assert_eq!(left.x, right.x, "{page:?} panes must stack\n{frame}");
        assert!(
            right.y >= left.y + left.height,
            "{page:?} pane overlap\n{frame}"
        );
    }
}

#[test]
fn quick_play_stacks_options_without_starving_songs() {
    let (mut daemon, mut client) = build_state();
    populate(&mut daemon);
    client.page = Page::QuickPlay;
    client.songs.selected_option = Some(SongOption::Starred);
    let frame = render(60, 42, &daemon, &mut client);
    let options = client.layout.content_left.expect("options pane");
    let songs = client.layout.content_right.expect("songs pane");
    assert_eq!(options.height, 6);
    assert_eq!(options.x, songs.x);
    assert!(
        songs.height > options.height,
        "song pane was starved\n{frame}"
    );
    assert!(frame.contains("A Song With A Readable Title"));
}

#[test]
fn narrow_short_layout_retains_content_and_now_playing() {
    let (mut daemon, mut client) = build_state();
    populate(&mut daemon);
    client.page = Page::QuickPlay;
    client.songs.selected_option = Some(SongOption::Starred);
    let frame = render(50, 18, &daemon, &mut client);
    assert!(
        client.layout.content.height > 0,
        "content collapsed\n{frame}"
    );
    assert!(
        client.layout.now_playing.height <= 4,
        "now-playing was not capped\n{frame}"
    );
    assert!(
        frame.contains("Song Options"),
        "options disappeared\n{frame}"
    );
}

#[test]
fn representative_responsive_renders() {
    for (name, page, width, height) in [
        ("wide", Page::Library, 120, 36),
        ("standard", Page::Playlists, 80, 24),
        ("narrow_tall", Page::Library, 60, 50),
        ("narrow_short", Page::QuickPlay, 50, 18),
    ] {
        let (mut daemon, mut client) = build_state();
        populate(&mut daemon);
        client.page = page;
        client.songs.selected_option = Some(SongOption::Starred);
        let frame = render(width, height, &daemon, &mut client);
        insta::assert_snapshot!(name, frame);
    }
}
