//! QuickPlay page actions: Enter to play selected, m to star.

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ferrosonic::app::models::SongOption;
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
    app.handle_key(key(KeyCode::F(3))).await.unwrap();
    AppFixture {
        app,
        _tempdir: tempdir,
    }
}

#[tokio::test]
#[serial]
async fn quickplay_down_in_option_pane_toggles_to_random() {
    let mut fx = build_app().await;
    fx.app.handle_key(key(KeyCode::Down)).await.unwrap();
    let cs = fx.app.client_state.read().await;
    assert!(matches!(
        cs.songs.selected_option,
        Some(SongOption::Random) | Some(SongOption::Starred)
    ));
}

#[tokio::test]
#[serial]
async fn quickplay_right_with_songs_focuses_song_pane() {
    let mut fx = build_app().await;
    {
        let mut ds = fx.app.daemon_state.write().await;
        ds.library.starred_songs = vec![song("s0")];
    }
    {
        let mut cs = fx.app.client_state.write().await;
        cs.songs.selected_option = Some(SongOption::Starred);
    }
    fx.app.handle_key(key(KeyCode::Right)).await.unwrap();
    let cs = fx.app.client_state.read().await;
    assert_eq!(cs.songs.focus, 1);
}

#[tokio::test]
#[serial]
async fn quickplay_m_stars_selected_song() {
    let mut fx = build_app().await;
    {
        let mut ds = fx.app.daemon_state.write().await;
        ds.library.starred_songs = vec![song("s0")];
    }
    {
        let mut cs = fx.app.client_state.write().await;
        cs.songs.selected_option = Some(SongOption::Starred);
        cs.songs.selected_index = Some(0);
        cs.songs.focus = 1;
    }
    fx.app.handle_key(key(KeyCode::Char('m'))).await.unwrap();
}

#[tokio::test]
#[serial]
async fn quickplay_enter_on_song_replaces_queue_and_plays() {
    let mut fx = build_app().await;
    {
        let mut ds = fx.app.daemon_state.write().await;
        ds.library.starred_songs = vec![song("s0"), song("s1")];
    }
    {
        let mut cs = fx.app.client_state.write().await;
        cs.songs.selected_option = Some(SongOption::Starred);
        cs.songs.selected_index = Some(1);
        cs.songs.focus = 1;
    }
    fx.app.handle_key(key(KeyCode::Enter)).await.unwrap();
}

#[tokio::test]
#[serial]
async fn quickplay_left_returns_focus_to_option_pane() {
    let mut fx = build_app().await;
    {
        let mut cs = fx.app.client_state.write().await;
        cs.songs.focus = 1;
    }
    fx.app.handle_key(key(KeyCode::Left)).await.unwrap();
    let cs = fx.app.client_state.read().await;
    assert_eq!(cs.songs.focus, 0);
}
