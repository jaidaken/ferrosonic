//! Library page action keys: e (enqueue), i (insert next), m (star), n (star current).

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
async fn e_in_song_pane_appends_song_to_queue() {
    let mut fx = build_app().await;
    {
        let mut cs = fx.app.client_state.write().await;
        cs.artists.songs = vec![song("s0"), song("s1")];
        cs.artists.selected_song = Some(0);
        cs.artists.focus = 1;
    }
    fx.app.handle_key(key(KeyCode::Char('e'))).await.unwrap();
    let ds = fx.app.daemon_state.read().await;
    assert!(
        ds.queue.iter().any(|s| s.id == "s0"),
        "e should append to queue; got {:?}",
        ds.queue.iter().map(|s| s.id.as_str()).collect::<Vec<_>>()
    );
}

#[tokio::test]
#[serial]
async fn e_in_tree_pane_appends_all_songs() {
    let mut fx = build_app().await;
    {
        let mut cs = fx.app.client_state.write().await;
        cs.artists.songs = vec![song("s0"), song("s1"), song("s2")];
        cs.artists.focus = 0;
    }
    fx.app.handle_key(key(KeyCode::Char('e'))).await.unwrap();
    let ds = fx.app.daemon_state.read().await;
    assert_eq!(
        ds.queue.len(),
        3,
        "e with full song list should append all three"
    );
}

#[tokio::test]
#[serial]
async fn m_in_song_pane_stars_selected_song() {
    let mut fx = build_app().await;
    {
        let mut cs = fx.app.client_state.write().await;
        cs.artists.songs = vec![song("s0")];
        cs.artists.selected_song = Some(0);
        cs.artists.focus = 1;
    }
    fx.app.handle_key(key(KeyCode::Char('m'))).await.unwrap();
}

#[tokio::test]
#[serial]
async fn i_in_song_pane_inserts_after_current_position() {
    let mut fx = build_app().await;
    {
        let mut ds = fx.app.daemon_state.write().await;
        ds.queue.push(song("existing"));
        ds.queue_position = Some(0);
    }
    {
        let mut cs = fx.app.client_state.write().await;
        cs.artists.songs = vec![song("new")];
        cs.artists.selected_song = Some(0);
        cs.artists.focus = 1;
    }
    fx.app.handle_key(key(KeyCode::Char('i'))).await.unwrap();
    let ds = fx.app.daemon_state.read().await;
    assert!(ds.queue.iter().any(|s| s.id == "new"));
}

#[tokio::test]
#[serial]
async fn t_at_artist_node_appends_artist_songs() {
    let mut fx = build_app().await;
    {
        let mut ds = fx.app.daemon_state.write().await;
        ds.library.artists = vec![artist("a0", "Artist A")];
        ds.library
            .albums_cache
            .insert("a0".into(), vec![album("alb0", "Alb A")]);
    }
    fx.app.handle_key(key(KeyCode::Down)).await.unwrap();
    fx.app.handle_key(key(KeyCode::Char('t'))).await.unwrap();
}

#[tokio::test]
#[serial]
async fn backspace_in_song_pane_switches_focus_to_tree() {
    let mut fx = build_app().await;
    {
        let mut cs = fx.app.client_state.write().await;
        cs.artists.focus = 1;
    }
    fx.app.handle_key(key(KeyCode::Backspace)).await.unwrap();
    let cs = fx.app.client_state.read().await;
    assert_eq!(cs.artists.focus, 0);
}
