//! Exhaustive input_songs.rs branches (Quick Play page).

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
async fn down_in_option_pane_starred_to_random_triggers_refresh() {
    let mut fx = build_app().await;
    {
        let mut cs = fx.app.client_state.write().await;
        cs.songs.focus = 0;
        cs.songs.selected_option = Some(SongOption::Starred);
    }
    fx.app.handle_key(key(KeyCode::Down)).await.unwrap();
    let cs = fx.app.client_state.read().await;
    assert!(matches!(cs.songs.selected_option, Some(SongOption::Random)));
}

#[tokio::test]
#[serial]
async fn down_at_random_option_is_noop() {
    let mut fx = build_app().await;
    {
        let mut cs = fx.app.client_state.write().await;
        cs.songs.focus = 0;
        cs.songs.selected_option = Some(SongOption::Random);
    }
    fx.app.handle_key(key(KeyCode::Down)).await.unwrap();
    assert!(matches!(
        fx.app.client_state.read().await.songs.selected_option,
        Some(SongOption::Random)
    ));
}

#[tokio::test]
#[serial]
async fn up_in_option_pane_random_to_starred_triggers_refresh() {
    let mut fx = build_app().await;
    {
        let mut cs = fx.app.client_state.write().await;
        cs.songs.focus = 0;
        cs.songs.selected_option = Some(SongOption::Random);
    }
    fx.app.handle_key(key(KeyCode::Up)).await.unwrap();
    assert!(matches!(
        fx.app.client_state.read().await.songs.selected_option,
        Some(SongOption::Starred)
    ));
}

#[tokio::test]
#[serial]
async fn up_at_starred_option_is_noop() {
    let mut fx = build_app().await;
    {
        let mut cs = fx.app.client_state.write().await;
        cs.songs.focus = 0;
        cs.songs.selected_option = Some(SongOption::Starred);
    }
    fx.app.handle_key(key(KeyCode::Up)).await.unwrap();
    assert!(matches!(
        fx.app.client_state.read().await.songs.selected_option,
        Some(SongOption::Starred)
    ));
}

#[tokio::test]
#[serial]
async fn up_with_no_option_selected_does_nothing() {
    let mut fx = build_app().await;
    {
        let mut cs = fx.app.client_state.write().await;
        cs.songs.focus = 0;
        cs.songs.selected_option = None;
    }
    fx.app.handle_key(key(KeyCode::Up)).await.unwrap();
}

#[tokio::test]
#[serial]
async fn down_in_song_pane_increments() {
    let mut fx = build_app().await;
    {
        let mut ds = fx.app.daemon_state.write().await;
        ds.library.random_songs = vec![song("a"), song("b")];
    }
    {
        let mut cs = fx.app.client_state.write().await;
        cs.songs.focus = 1;
        cs.songs.selected_option = Some(SongOption::Random);
        cs.songs.selected_index = Some(0);
    }
    fx.app.handle_key(key(KeyCode::Down)).await.unwrap();
    assert_eq!(
        fx.app.client_state.read().await.songs.selected_index,
        Some(1)
    );
}

#[tokio::test]
#[serial]
async fn up_in_song_pane_decrements() {
    let mut fx = build_app().await;
    {
        let mut ds = fx.app.daemon_state.write().await;
        ds.library.random_songs = vec![song("a"), song("b"), song("c")];
    }
    {
        let mut cs = fx.app.client_state.write().await;
        cs.songs.focus = 1;
        cs.songs.selected_option = Some(SongOption::Random);
        cs.songs.selected_index = Some(2);
    }
    fx.app.handle_key(key(KeyCode::Up)).await.unwrap();
    assert_eq!(
        fx.app.client_state.read().await.songs.selected_index,
        Some(1)
    );
}

#[tokio::test]
#[serial]
async fn down_in_song_pane_initializes_with_no_selection() {
    let mut fx = build_app().await;
    {
        let mut ds = fx.app.daemon_state.write().await;
        ds.library.random_songs = vec![song("a")];
    }
    {
        let mut cs = fx.app.client_state.write().await;
        cs.songs.focus = 1;
        cs.songs.selected_option = Some(SongOption::Random);
    }
    fx.app.handle_key(key(KeyCode::Down)).await.unwrap();
    assert_eq!(
        fx.app.client_state.read().await.songs.selected_index,
        Some(0)
    );
}

#[tokio::test]
#[serial]
async fn enter_with_valid_index_plays_song() {
    let mut fx = build_app().await;
    {
        let mut ds = fx.app.daemon_state.write().await;
        ds.library.random_songs = vec![song("rs0"), song("rs1")];
    }
    {
        let mut cs = fx.app.client_state.write().await;
        cs.songs.focus = 1;
        cs.songs.selected_option = Some(SongOption::Random);
        cs.songs.selected_index = Some(1);
    }
    fx.app.handle_key(key(KeyCode::Enter)).await.unwrap();
    let ds = fx.app.daemon_state.read().await;
    assert!(ds.queue.iter().any(|s| s.id == "rs1"));
}

#[tokio::test]
#[serial]
async fn enter_with_oob_index_is_noop() {
    let mut fx = build_app().await;
    {
        let mut cs = fx.app.client_state.write().await;
        cs.songs.focus = 1;
        cs.songs.selected_option = Some(SongOption::Random);
        cs.songs.selected_index = Some(99);
    }
    fx.app.handle_key(key(KeyCode::Enter)).await.unwrap();
    let ds = fx.app.daemon_state.read().await;
    assert!(ds.queue.is_empty());
}

#[tokio::test]
#[serial]
async fn enter_with_no_selection_is_noop() {
    let mut fx = build_app().await;
    fx.app.handle_key(key(KeyCode::Enter)).await.unwrap();
    let ds = fx.app.daemon_state.read().await;
    assert!(ds.queue.is_empty());
}

#[tokio::test]
#[serial]
async fn tab_toggles_focus_zero_to_one() {
    let mut fx = build_app().await;
    fx.app.handle_key(key(KeyCode::Tab)).await.unwrap();
    assert_eq!(fx.app.client_state.read().await.songs.focus, 1);
}

#[tokio::test]
#[serial]
async fn tab_toggles_focus_one_to_zero() {
    let mut fx = build_app().await;
    {
        let mut cs = fx.app.client_state.write().await;
        cs.songs.focus = 1;
    }
    fx.app.handle_key(key(KeyCode::Tab)).await.unwrap();
    assert_eq!(fx.app.client_state.read().await.songs.focus, 0);
}

#[tokio::test]
#[serial]
async fn left_forces_focus_to_zero() {
    let mut fx = build_app().await;
    {
        let mut cs = fx.app.client_state.write().await;
        cs.songs.focus = 1;
    }
    fx.app.handle_key(key(KeyCode::Left)).await.unwrap();
    assert_eq!(fx.app.client_state.read().await.songs.focus, 0);
}

#[tokio::test]
#[serial]
async fn right_with_no_songs_is_noop() {
    let mut fx = build_app().await;
    fx.app.handle_key(key(KeyCode::Right)).await.unwrap();
    assert_eq!(fx.app.client_state.read().await.songs.focus, 0);
}

#[tokio::test]
#[serial]
async fn right_with_songs_focuses_one() {
    let mut fx = build_app().await;
    {
        let mut ds = fx.app.daemon_state.write().await;
        ds.library.random_songs = vec![song("a")];
    }
    {
        let mut cs = fx.app.client_state.write().await;
        cs.songs.selected_option = Some(SongOption::Random);
    }
    fx.app.handle_key(key(KeyCode::Right)).await.unwrap();
    let cs = fx.app.client_state.read().await;
    assert_eq!(cs.songs.focus, 1);
    assert_eq!(cs.songs.selected_index, Some(0));
}

#[tokio::test]
#[serial]
async fn m_with_no_selection_is_noop() {
    let mut fx = build_app().await;
    fx.app.handle_key(key(KeyCode::Char('m'))).await.unwrap();
}

#[tokio::test]
#[serial]
async fn m_with_valid_selection_stars_song() {
    let mut fx = build_app().await;
    {
        let mut ds = fx.app.daemon_state.write().await;
        ds.library.random_songs = vec![song("starme")];
    }
    {
        let mut cs = fx.app.client_state.write().await;
        cs.songs.focus = 1;
        cs.songs.selected_option = Some(SongOption::Random);
        cs.songs.selected_index = Some(0);
    }
    fx.app.handle_key(key(KeyCode::Char('m'))).await.unwrap();
}

#[tokio::test]
#[serial]
async fn unhandled_key_is_noop() {
    let mut fx = build_app().await;
    fx.app.handle_key(key(KeyCode::Insert)).await.unwrap();
}
