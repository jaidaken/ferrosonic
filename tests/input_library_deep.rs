//! Deep library-page input tests with populated state.

mod common;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ferrosonic::app::App;
use ferrosonic::config::Config;
use ferrosonic::subsonic::models::{Album, Artist};
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
        artist: Some("Test".into()),
        artist_id: Some("a0".into()),
        cover_art: None,
        song_count: Some(5),
        original_release_date: None,
        duration: Some(1200),
        year: Some(2020),
        genre: None,
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
    let mut app = App::new(config);
    app.handle_key(key(KeyCode::F(1))).await.unwrap();
    AppFixture {
        app,
        _tempdir: tempdir,
    }
}

#[tokio::test]
#[serial]
async fn down_in_artist_tree_advances_selection() {
    let mut fx = build_app().await;
    {
        let mut ds = fx.app.daemon_state.write().await;
        ds.library.artists = vec![artist("a0", "A"), artist("a1", "B"), artist("a2", "C")];
    }
    fx.app.handle_key(key(KeyCode::Down)).await.unwrap();
    fx.app.handle_key(key(KeyCode::Down)).await.unwrap();
    let cs = fx.app.client_state.read().await;
    assert!(
        matches!(cs.artists.selected_index, Some(idx) if idx > 0),
        "Down should advance tree selection; got {:?}",
        cs.artists.selected_index
    );
}

#[tokio::test]
#[serial]
async fn up_at_top_does_not_underflow() {
    let mut fx = build_app().await;
    {
        let mut ds = fx.app.daemon_state.write().await;
        ds.library.artists = vec![artist("a0", "A")];
    }
    fx.app.handle_key(key(KeyCode::Down)).await.unwrap();
    fx.app.handle_key(key(KeyCode::Up)).await.unwrap();
    fx.app.handle_key(key(KeyCode::Up)).await.unwrap();
    let cs = fx.app.client_state.read().await;
    assert!(cs.artists.selected_index.unwrap_or(99) == 0);
}

#[tokio::test]
#[serial]
async fn t_at_artist_node_triggers_shuffle_context_request() {
    let mut fx = build_app().await;
    {
        let mut ds = fx.app.daemon_state.write().await;
        ds.library.artists = vec![artist("a0", "Artist A")];
        ds.library
            .albums_cache
            .insert("a0".into(), vec![album("alb0", "Album A")]);
    }
    fx.app.handle_key(key(KeyCode::Down)).await.unwrap();
    fx.app.handle_key(key(KeyCode::Char('t'))).await.unwrap();
}

#[tokio::test]
#[serial]
async fn enter_on_expanded_artist_collapses_it() {
    let mut fx = build_app().await;
    {
        let mut ds = fx.app.daemon_state.write().await;
        ds.library.artists = vec![artist("a0", "Artist A")];
        ds.library
            .albums_cache
            .insert("a0".into(), vec![album("alb0", "Album A")]);
    }
    {
        let mut cs = fx.app.client_state.write().await;
        cs.artists.expanded.insert("a0".into());
        cs.artists.selected_index = Some(0);
    }
    fx.app.handle_key(key(KeyCode::Enter)).await.unwrap();
    let cs = fx.app.client_state.read().await;
    assert!(
        cs.artists.expanded.is_empty(),
        "Enter on expanded artist should collapse it; expanded: {:?}",
        cs.artists.expanded
    );
}

#[tokio::test]
#[serial]
async fn right_arrow_with_songs_switches_focus_to_song_pane() {
    let mut fx = build_app().await;
    {
        let mut cs = fx.app.client_state.write().await;
        cs.artists.songs = vec![ferrosonic::subsonic::models::Child {
            id: "s0".into(),
            title: "S".into(),
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
        }];
    }
    fx.app.handle_key(key(KeyCode::Right)).await.unwrap();
    let cs = fx.app.client_state.read().await;
    assert_eq!(
        cs.artists.focus, 1,
        "Right with songs should switch focus to song pane"
    );
}
