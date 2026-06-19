//! Mouse clicks on library page tree + songs panes.

mod common;
use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ferrosonic::app::state::Page;
use ferrosonic::app::App;
use ferrosonic::config::Config;
use ferrosonic::subsonic::models::{Album, Artist, Child};
use ratatui::layout::Rect;
use serial_test::serial;

fn artist(id: &str, name: &str) -> Artist {
    Artist {
        id: id.into(),
        name: name.into(),
        album_count: Some(1),
        cover_art: None,
    }
}

fn album(id: &str, name: &str) -> Album {
    Album {
        id: id.into(),
        name: name.into(),
        artist: Some("X".into()),
        artist_id: Some("a0".into()),
        cover_art: None,
        song_count: Some(3),
        original_release_date: None,
        duration: Some(540),
        year: Some(2020),
        genre: None,
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
        cs.page = Page::Library;
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
async fn click_on_tree_pane_row_selects_that_artist() {
    let mut fx = build_app().await;
    {
        let mut ds = fx.app.daemon_state.write().await;
        ds.library.artists = vec![
            artist("a0", "Alpha"),
            artist("a1", "Bravo"),
            artist("a2", "Charlie"),
        ];
    }
    fx.app.handle_mouse(click(10, 3)).await.unwrap();
    let cs = fx.app.client_state.read().await;
    assert!(
        cs.artists.selected_index.is_some(),
        "tree click should select something"
    );
    assert_eq!(cs.artists.focus, 0, "tree click focuses left pane");
}

#[tokio::test]
#[serial]
async fn click_on_song_pane_row_focuses_songs() {
    let mut fx = build_app().await;
    {
        let mut cs = fx.app.client_state.write().await;
        cs.artists.songs = vec![song("s0"), song("s1"), song("s2")];
    }
    fx.app.handle_mouse(click(50, 3)).await.unwrap();
    let cs = fx.app.client_state.read().await;
    assert_eq!(cs.artists.focus, 1, "song pane click focuses right pane");
}

#[tokio::test]
#[serial]
async fn double_click_on_artist_expands_collapses() {
    let mut fx = build_app().await;
    {
        let mut ds = fx.app.daemon_state.write().await;
        ds.library.artists = vec![artist("a0", "Artist")];
        ds.library
            .albums_cache
            .insert("a0".into(), vec![album("alb0", "Album")]);
    }
    fx.app.handle_mouse(click(10, 2)).await.unwrap();
    fx.app.handle_mouse(click(10, 2)).await.unwrap();
    // No assertion on side effect; mainly verify no panic on double-click path.
}

#[tokio::test]
#[serial]
async fn click_outside_content_areas_is_safe() {
    let mut fx = build_app().await;
    {
        let mut ds = fx.app.daemon_state.write().await;
        ds.library.artists = vec![artist("a0", "X")];
    }
    fx.app.handle_mouse(click(200, 200)).await.unwrap();
}

#[tokio::test]
#[serial]
async fn click_on_empty_tree_is_noop() {
    let mut fx = build_app().await;
    fx.app.handle_mouse(click(10, 5)).await.unwrap();
    let cs = fx.app.client_state.read().await;
    assert!(cs.artists.selected_index.is_none() || cs.artists.selected_index == Some(0));
}
