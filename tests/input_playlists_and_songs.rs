//! Playlists + QuickPlay page deep input.

mod common;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ferrosonic::app::state::Page;
use ferrosonic::app::App;
use ferrosonic::config::Config;
use ferrosonic::subsonic::models::{Child, Playlist};
use serial_test::serial;

fn key(code: KeyCode) -> KeyEvent {
    let mut k = KeyEvent::new(code, KeyModifiers::NONE);
    k.kind = KeyEventKind::Press;
    k
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

fn playlist(id: &str, name: &str) -> Playlist {
    Playlist {
        id: id.into(),
        name: name.into(),
        comment: None,
        owner: None,
        public: None,
        song_count: Some(5),
        duration: Some(900),
        cover_art: None,
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
    AppFixture {
        app,
        _tempdir: tempdir,
    }
}

#[tokio::test]
#[serial]
async fn playlists_down_advances_through_playlists() {
    let mut fx = build_app().await;
    {
        let mut ds = fx.app.daemon_state.write().await;
        ds.library.playlists = vec![
            playlist("p0", "A"),
            playlist("p1", "B"),
            playlist("p2", "C"),
        ];
    }
    fx.app.handle_key(key(KeyCode::F(4))).await.unwrap();
    fx.app.handle_key(key(KeyCode::Down)).await.unwrap();
    fx.app.handle_key(key(KeyCode::Down)).await.unwrap();
    let cs = fx.app.client_state.read().await;
    assert_eq!(cs.page, Page::Playlists);
    assert!(
        cs.playlists.selected_playlist.unwrap_or(99) > 0,
        "Down should advance selection; got {:?}",
        cs.playlists.selected_playlist
    );
}

#[tokio::test]
#[serial]
async fn playlists_up_at_top_bounds_to_zero() {
    let mut fx = build_app().await;
    {
        let mut ds = fx.app.daemon_state.write().await;
        ds.library.playlists = vec![playlist("p0", "A"), playlist("p1", "B")];
    }
    fx.app.handle_key(key(KeyCode::F(4))).await.unwrap();
    fx.app.handle_key(key(KeyCode::Down)).await.unwrap();
    fx.app.handle_key(key(KeyCode::Up)).await.unwrap();
    fx.app.handle_key(key(KeyCode::Up)).await.unwrap();
    let cs = fx.app.client_state.read().await;
    assert!(cs.playlists.selected_playlist.unwrap_or(99) == 0);
}

#[tokio::test]
#[serial]
async fn playlists_right_with_songs_switches_focus_to_songs_pane() {
    let mut fx = build_app().await;
    fx.app.handle_key(key(KeyCode::F(4))).await.unwrap();
    {
        let mut cs = fx.app.client_state.write().await;
        cs.playlists.songs = vec![song("s0")];
    }
    fx.app.handle_key(key(KeyCode::Right)).await.unwrap();
    let cs = fx.app.client_state.read().await;
    assert_eq!(cs.playlists.focus, 1);
}

#[tokio::test]
#[serial]
async fn playlists_tab_cycles_focus() {
    let mut fx = build_app().await;
    fx.app.handle_key(key(KeyCode::F(4))).await.unwrap();
    let initial = fx.app.client_state.read().await.playlists.focus;
    fx.app.handle_key(key(KeyCode::Tab)).await.unwrap();
    let after = fx.app.client_state.read().await.playlists.focus;
    assert_ne!(initial, after);
}

#[tokio::test]
#[serial]
async fn quickplay_left_arrow_focuses_option_list() {
    let mut fx = build_app().await;
    fx.app.handle_key(key(KeyCode::F(3))).await.unwrap();
    {
        let mut cs = fx.app.client_state.write().await;
        cs.songs.focus = 1;
    }
    fx.app.handle_key(key(KeyCode::Left)).await.unwrap();
    let cs = fx.app.client_state.read().await;
    assert_eq!(cs.songs.focus, 0);
}

#[tokio::test]
#[serial]
async fn quickplay_tab_cycles_focus() {
    let mut fx = build_app().await;
    fx.app.handle_key(key(KeyCode::F(3))).await.unwrap();
    let initial = fx.app.client_state.read().await.songs.focus;
    fx.app.handle_key(key(KeyCode::Tab)).await.unwrap();
    let after = fx.app.client_state.read().await.songs.focus;
    assert_ne!(initial, after);
}
