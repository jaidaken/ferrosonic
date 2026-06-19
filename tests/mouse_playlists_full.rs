//! Mouse clicks on playlists page tree + songs panes.

mod common;
use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ferrosonic::app::state::Page;
use ferrosonic::app::App;
use ferrosonic::config::Config;
use ferrosonic::subsonic::models::{Child, Playlist};
use ratatui::layout::Rect;
use serial_test::serial;

fn playlist(id: &str, name: &str) -> Playlist {
    Playlist {
        id: id.into(),
        name: name.into(),
        owner: Some("u".into()),
        song_count: Some(10),
        duration: Some(1800),
        cover_art: None,
        public: Some(false),
        comment: None,
    }
}

fn song(id: &str) -> Child {
    Child {
        id: id.into(),
        title: id.into(),
        parent: None,
        is_dir: false,
        album: None,
        artist: None,
        artist_id: None,
        album_id: None,
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

fn click(x: u16, y: u16) -> MouseEvent {
    MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: x,
        row: y,
        modifiers: KeyModifiers::NONE,
    }
}

struct AppFixture {
    app: App,
    _tempdir: tempfile::TempDir,
}

async fn build_app() -> AppFixture {
    let tempdir = common::tempdir();
    std::env::set_var("FERROSONIC_CONFIG_DIR", tempdir.path());
    let mut config = Config::new();
    config.daemon = false;
    let app = App::new(config);
    {
        let mut cs = app.client_state.write().await;
        cs.page = Page::Playlists;
        cs.layout.header = Rect::new(0, 0, 80, 1);
        cs.layout.content = Rect::new(0, 1, 80, 20);
        cs.layout.content_left = Some(Rect::new(0, 1, 40, 20));
        cs.layout.content_right = Some(Rect::new(40, 1, 40, 20));
        cs.layout.now_playing = Rect::new(0, 21, 80, 7);
    }
    AppFixture {
        app,
        _tempdir: tempdir,
    }
}

#[tokio::test]
#[serial]
async fn single_click_on_playlist_selects_focus_zero() {
    let mut fx = build_app().await;
    {
        let mut ds = fx.app.daemon_state.write().await;
        ds.library.playlists = vec![playlist("p0", "A"), playlist("p1", "B")];
    }
    fx.app.handle_mouse(click(10, 2)).await.unwrap();
    let cs = fx.app.client_state.read().await;
    assert_eq!(cs.playlists.selected_playlist, Some(0));
    assert_eq!(cs.playlists.focus, 0);
}

#[tokio::test]
#[serial]
async fn second_click_on_same_playlist_loads_songs() {
    let mut fx = build_app().await;
    {
        let mut ds = fx.app.daemon_state.write().await;
        ds.library.playlists = vec![playlist("p0", "Mix")];
    }
    fx.app.handle_mouse(click(10, 2)).await.unwrap();
    fx.app.handle_mouse(click(10, 2)).await.unwrap();
}

#[tokio::test]
#[serial]
async fn click_beyond_playlists_is_noop() {
    let mut fx = build_app().await;
    {
        let mut ds = fx.app.daemon_state.write().await;
        ds.library.playlists = vec![playlist("p0", "A")];
    }
    fx.app.handle_mouse(click(10, 15)).await.unwrap();
    let cs = fx.app.client_state.read().await;
    assert!(cs.playlists.selected_playlist.is_none());
}

#[tokio::test]
#[serial]
async fn click_on_song_pane_selects_song_focus_one() {
    let mut fx = build_app().await;
    {
        let mut cs = fx.app.client_state.write().await;
        cs.playlists.songs = vec![song("s0"), song("s1")];
    }
    fx.app.handle_mouse(click(50, 2)).await.unwrap();
    let cs = fx.app.client_state.read().await;
    assert_eq!(cs.playlists.selected_song, Some(0));
    assert_eq!(cs.playlists.focus, 1);
}

#[tokio::test]
#[serial]
async fn second_click_on_same_song_plays_from_index() {
    let mut fx = build_app().await;
    {
        let mut cs = fx.app.client_state.write().await;
        cs.playlists.songs = vec![song("s0"), song("s1"), song("s2")];
    }
    fx.app.handle_mouse(click(50, 3)).await.unwrap();
    fx.app.handle_mouse(click(50, 3)).await.unwrap();
}

#[tokio::test]
#[serial]
async fn click_beyond_song_pane_rows_is_noop() {
    let mut fx = build_app().await;
    {
        let mut cs = fx.app.client_state.write().await;
        cs.playlists.songs = vec![song("s0")];
    }
    fx.app.handle_mouse(click(50, 15)).await.unwrap();
}

#[tokio::test]
#[serial]
async fn click_outside_both_panes_is_safe() {
    let mut fx = build_app().await;
    fx.app.handle_mouse(click(200, 200)).await.unwrap();
}

#[tokio::test]
#[serial]
async fn playlist_scroll_offset_shifts_click_target() {
    let mut fx = build_app().await;
    {
        let mut ds = fx.app.daemon_state.write().await;
        ds.library.playlists = (0..30).map(|i| playlist(&format!("p{i}"), "x")).collect();
    }
    {
        let mut cs = fx.app.client_state.write().await;
        cs.playlists.playlist_scroll_offset = 8;
    }
    fx.app.handle_mouse(click(10, 3)).await.unwrap();
    let cs = fx.app.client_state.read().await;
    assert_eq!(cs.playlists.selected_playlist, Some(9));
}
