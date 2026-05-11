//! Playlists page action keys: e, i, t, m.

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ferrosonic::app::App;
use ferrosonic::config::Config;
use ferrosonic::subsonic::models::Child;
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
    app.handle_key(key(KeyCode::F(4))).await.unwrap();
    AppFixture {
        app,
        _tempdir: tempdir,
    }
}

#[tokio::test]
#[serial]
async fn e_in_playlist_song_pane_appends_selected_song() {
    let mut fx = build_app().await;
    {
        let mut cs = fx.app.client_state.write().await;
        cs.playlists.songs = vec![song("s0"), song("s1")];
        cs.playlists.selected_song = Some(1);
        cs.playlists.focus = 1;
    }
    fx.app.handle_key(key(KeyCode::Char('e'))).await.unwrap();
    let ds = fx.app.daemon_state.read().await;
    assert!(ds.queue.iter().any(|s| s.id == "s1"));
}

#[tokio::test]
#[serial]
async fn e_in_playlist_tree_pane_appends_all_playlist_songs() {
    let mut fx = build_app().await;
    {
        let mut cs = fx.app.client_state.write().await;
        cs.playlists.songs = vec![song("s0"), song("s1"), song("s2")];
        cs.playlists.focus = 0;
    }
    fx.app.handle_key(key(KeyCode::Char('e'))).await.unwrap();
    let ds = fx.app.daemon_state.read().await;
    assert_eq!(ds.queue.len(), 3);
}

#[tokio::test]
#[serial]
async fn i_in_playlist_song_pane_inserts_after_current() {
    let mut fx = build_app().await;
    {
        let mut ds = fx.app.daemon_state.write().await;
        ds.queue.push(song("existing"));
        ds.queue_position = Some(0);
    }
    {
        let mut cs = fx.app.client_state.write().await;
        cs.playlists.songs = vec![song("new")];
        cs.playlists.selected_song = Some(0);
        cs.playlists.focus = 1;
    }
    fx.app.handle_key(key(KeyCode::Char('i'))).await.unwrap();
    let ds = fx.app.daemon_state.read().await;
    assert!(ds.queue.iter().any(|s| s.id == "new"));
}

#[tokio::test]
#[serial]
async fn t_on_playlist_replaces_queue_with_shuffled_songs() {
    let mut fx = build_app().await;
    {
        let mut cs = fx.app.client_state.write().await;
        cs.playlists.songs = vec![song("s0"), song("s1"), song("s2")];
    }
    fx.app.handle_key(key(KeyCode::Char('t'))).await.unwrap();
    let ds = fx.app.daemon_state.read().await;
    assert_eq!(ds.queue.len(), 3, "t should replace queue with 3 songs");
}

#[tokio::test]
#[serial]
async fn m_in_playlist_song_pane_toggles_star_on_selected() {
    let mut fx = build_app().await;
    {
        let mut cs = fx.app.client_state.write().await;
        cs.playlists.songs = vec![song("s0")];
        cs.playlists.selected_song = Some(0);
        cs.playlists.focus = 1;
    }
    fx.app.handle_key(key(KeyCode::Char('m'))).await.unwrap();
}
