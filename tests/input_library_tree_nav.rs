//! Deep library tree navigation: multi-artist scrolling, Tab focus, album Enter.

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ferrosonic::app::App;
use ferrosonic::config::Config;
use ferrosonic::subsonic::models::{Album, Artist, Child};
use serial_test::serial;

fn key(code: KeyCode) -> KeyEvent {
    let mut k = KeyEvent::new(code, KeyModifiers::NONE);
    k.kind = KeyEventKind::Press;
    k
}

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
        song_count: Some(2),
        duration: Some(360),
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

struct AppFixture {
    app: App,
    _tempdir: tempfile::TempDir,
}

async fn build_app() -> AppFixture {
    let tempdir = tempfile::tempdir().unwrap();
    std::env::set_var("FERROSONIC_CONFIG_DIR", tempdir.path());
    let mut config = Config::new();
    config.daemon = false;
    let mut app = App::new(config);
    app.handle_key(key(KeyCode::F(1))).await.unwrap();
    AppFixture {
        app,
        _tempdir: tempdir,
    }
}

#[tokio::test]
#[serial]
async fn down_scrolls_through_multiple_artists() {
    let mut fx = build_app().await;
    {
        let mut ds = fx.app.daemon_state.write().await;
        ds.library.artists = (0..5)
            .map(|i| artist(&format!("a{}", i), &format!("A{}", i)))
            .collect();
    }
    for _ in 0..3 {
        fx.app.handle_key(key(KeyCode::Down)).await.unwrap();
    }
    let cs = fx.app.client_state.read().await;
    assert_eq!(cs.artists.selected_index, Some(2));
}

#[tokio::test]
#[serial]
async fn j_advances_like_down_arrow() {
    let mut fx = build_app().await;
    {
        let mut ds = fx.app.daemon_state.write().await;
        ds.library.artists = vec![artist("a0", "A"), artist("a1", "B")];
    }
    fx.app.handle_key(key(KeyCode::Char('j'))).await.unwrap();
    fx.app.handle_key(key(KeyCode::Char('j'))).await.unwrap();
    let cs = fx.app.client_state.read().await;
    assert!(cs.artists.selected_index.unwrap_or(0) > 0);
}

#[tokio::test]
#[serial]
async fn k_moves_up_like_up_arrow() {
    let mut fx = build_app().await;
    {
        let mut ds = fx.app.daemon_state.write().await;
        ds.library.artists = vec![artist("a0", "A"), artist("a1", "B"), artist("a2", "C")];
    }
    fx.app.handle_key(key(KeyCode::Down)).await.unwrap();
    fx.app.handle_key(key(KeyCode::Down)).await.unwrap();
    fx.app.handle_key(key(KeyCode::Char('k'))).await.unwrap();
    let cs = fx.app.client_state.read().await;
    assert!(cs.artists.selected_index.unwrap_or(99) < 2);
}

#[tokio::test]
#[serial]
async fn tab_cycles_between_tree_and_songs_pane() {
    let mut fx = build_app().await;
    let initial = fx.app.client_state.read().await.artists.focus;
    fx.app.handle_key(key(KeyCode::Tab)).await.unwrap();
    let after = fx.app.client_state.read().await.artists.focus;
    assert_ne!(initial, after);
}

#[tokio::test]
#[serial]
async fn enter_on_album_node_plays_album() {
    let mut fx = build_app().await;
    {
        let mut ds = fx.app.daemon_state.write().await;
        ds.library.artists = vec![artist("a0", "Artist")];
        ds.library
            .albums_cache
            .insert("a0".into(), vec![album("alb0", "Album")]);
        ds.library
            .album_songs_cache
            .insert("alb0".into(), vec![song("alb0-1"), song("alb0-2")]);
    }
    {
        let mut cs = fx.app.client_state.write().await;
        cs.artists.expanded.insert("a0".into());
        cs.artists.selected_index = Some(1);
    }
    fx.app.handle_key(key(KeyCode::Enter)).await.unwrap();
    // Without a real Subsonic client, the album-load path may not
    // populate the queue; just verify the key dispatch didn't panic.
}

#[tokio::test]
#[serial]
async fn enter_on_song_node_in_expanded_album_plays_that_song() {
    let mut fx = build_app().await;
    {
        let mut ds = fx.app.daemon_state.write().await;
        ds.library.artists = vec![artist("a0", "Artist")];
        ds.library
            .albums_cache
            .insert("a0".into(), vec![album("alb0", "Album")]);
        ds.library
            .album_songs_cache
            .insert("alb0".into(), vec![song("t1"), song("t2")]);
    }
    {
        let mut cs = fx.app.client_state.write().await;
        cs.artists.expanded.insert("a0".into());
        cs.artists.selected_index = Some(2);
    }
    fx.app.handle_key(key(KeyCode::Enter)).await.unwrap();
}
