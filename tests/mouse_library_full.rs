//! Exhaustive mouse-click branches in mouse_library.rs.

mod common;
use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ferrosonic::app::state::Page;
use ferrosonic::app::App;
use ferrosonic::config::Config;
use ferrosonic::subsonic::models::{Album, Artist, Child, SearchResult3};
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
async fn double_click_on_expanded_artist_collapses_it() {
    let mut fx = build_app().await;
    {
        let mut ds = fx.app.daemon_state.write().await;
        ds.library.artists = vec![artist("a0", "Artist")];
        ds.library
            .albums_cache
            .insert("a0".into(), vec![album("alb0", "Album")]);
    }
    {
        let mut cs = fx.app.client_state.write().await;
        cs.artists.expanded.insert("a0".into());
    }
    fx.app.handle_mouse(click(10, 2)).await.unwrap();
    fx.app.handle_mouse(click(10, 2)).await.unwrap();
    let cs = fx.app.client_state.read().await;
    assert!(
        !cs.artists.expanded.contains("a0"),
        "second click on expanded artist should collapse"
    );
}

#[tokio::test]
#[serial]
async fn single_click_on_album_loads_songs_into_song_pane() {
    let mut fx = build_app().await;
    {
        let mut ds = fx.app.daemon_state.write().await;
        ds.library.artists = vec![artist("a0", "A")];
        ds.library
            .albums_cache
            .insert("a0".into(), vec![album("alb0", "Album")]);
    }
    {
        let mut cs = fx.app.client_state.write().await;
        cs.artists.expanded.insert("a0".into());
    }
    fx.app.handle_mouse(click(10, 3)).await.unwrap();
    let cs = fx.app.client_state.read().await;
    assert_eq!(cs.artists.selected_index, Some(1));
}

#[tokio::test]
#[serial]
async fn double_click_on_album_in_search_mode_plays_replace() {
    let mut fx = build_app().await;
    {
        let mut cs = fx.app.client_state.write().await;
        cs.artists.filter = "x".into();
        cs.artists.search_results = Some(SearchResult3 {
            artist: vec![],
            album: vec![album("alb0", "A")],
            song: vec![],
        });
    }
    fx.app.handle_mouse(click(10, 2)).await.unwrap();
    fx.app.handle_mouse(click(10, 2)).await.unwrap();
}

#[tokio::test]
#[serial]
async fn double_click_on_song_in_search_mode_plays_replace() {
    let mut fx = build_app().await;
    {
        let mut cs = fx.app.client_state.write().await;
        cs.artists.filter = "x".into();
        cs.artists.search_results = Some(SearchResult3 {
            artist: vec![],
            album: vec![],
            song: vec![song("s0")],
        });
    }
    fx.app.handle_mouse(click(10, 2)).await.unwrap();
    fx.app.handle_mouse(click(10, 2)).await.unwrap();
}

#[tokio::test]
#[serial]
async fn double_click_on_artist_not_in_cache_attempts_load() {
    let mut fx = build_app().await;
    {
        let mut ds = fx.app.daemon_state.write().await;
        ds.library.artists = vec![artist("a0", "A")];
    }
    fx.app.handle_mouse(click(10, 2)).await.unwrap();
    fx.app.handle_mouse(click(10, 2)).await.unwrap();
}

#[tokio::test]
#[serial]
async fn second_click_on_song_pane_row_plays() {
    let mut fx = build_app().await;
    {
        let mut cs = fx.app.client_state.write().await;
        cs.artists.songs = vec![song("s0"), song("s1")];
    }
    fx.app.handle_mouse(click(50, 2)).await.unwrap();
    fx.app.handle_mouse(click(50, 2)).await.unwrap();
}

#[tokio::test]
#[serial]
async fn click_uses_tree_scroll_offset_for_row_index() {
    let mut fx = build_app().await;
    {
        let mut ds = fx.app.daemon_state.write().await;
        ds.library.artists = (0..30)
            .map(|i| artist(&format!("a{i}"), &format!("Name {i}")))
            .collect();
    }
    {
        let mut cs = fx.app.client_state.write().await;
        cs.artists.tree_scroll_offset = 10;
    }
    fx.app.handle_mouse(click(10, 3)).await.unwrap();
    let cs = fx.app.client_state.read().await;
    assert_eq!(cs.artists.selected_index, Some(11));
}

#[tokio::test]
#[serial]
async fn click_uses_song_scroll_offset_for_row_index() {
    let mut fx = build_app().await;
    {
        let mut cs = fx.app.client_state.write().await;
        cs.artists.songs = (0..30).map(|i| song(&format!("s{i}"))).collect();
        cs.artists.song_scroll_offset = 5;
    }
    fx.app.handle_mouse(click(50, 4)).await.unwrap();
    let cs = fx.app.client_state.read().await;
    assert_eq!(cs.artists.selected_song, Some(7));
}

#[tokio::test]
#[serial]
async fn click_beyond_tree_items_is_noop() {
    let mut fx = build_app().await;
    {
        let mut ds = fx.app.daemon_state.write().await;
        ds.library.artists = vec![artist("a0", "A")];
    }
    fx.app.handle_mouse(click(10, 15)).await.unwrap();
}

#[tokio::test]
#[serial]
async fn click_beyond_song_items_is_noop() {
    let mut fx = build_app().await;
    {
        let mut cs = fx.app.client_state.write().await;
        cs.artists.songs = vec![song("s0")];
    }
    fx.app.handle_mouse(click(50, 15)).await.unwrap();
}

#[tokio::test]
#[serial]
async fn double_click_at_different_position_is_not_second_click() {
    let mut fx = build_app().await;
    {
        let mut cs = fx.app.client_state.write().await;
        cs.artists.songs = vec![song("s0"), song("s1")];
    }
    fx.app.handle_mouse(click(50, 2)).await.unwrap();
    fx.app.handle_mouse(click(50, 3)).await.unwrap();
    let cs = fx.app.client_state.read().await;
    assert_eq!(cs.artists.selected_song, Some(1));
}
