//! Exhaustive playlists-page key handlers.

mod common;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ferrosonic::app::App;
use ferrosonic::config::Config;
use ferrosonic::subsonic::models::{Child, Playlist};
use serial_test::serial;

fn key(code: KeyCode) -> KeyEvent {
    let mut k = KeyEvent::new(code, KeyModifiers::NONE);
    k.kind = KeyEventKind::Press;
    k
}

fn playlist(id: &str, name: &str) -> Playlist {
    Playlist {
        id: id.into(),
        name: name.into(),
        owner: Some("u".into()),
        song_count: Some(5),
        duration: Some(1200),
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
    app.handle_key(key(KeyCode::F(4))).await.unwrap();
    AppFixture {
        app,
        _tempdir: tempdir,
    }
}

#[tokio::test]
#[serial]
async fn tab_cycles_focus_between_panes() {
    let mut fx = build_app().await;
    fx.app.handle_key(key(KeyCode::Tab)).await.unwrap();
    assert_eq!(fx.app.client_state.read().await.playlists.focus, 1);
    fx.app.handle_key(key(KeyCode::Tab)).await.unwrap();
    assert_eq!(fx.app.client_state.read().await.playlists.focus, 0);
}

#[tokio::test]
#[serial]
async fn left_forces_focus_to_zero() {
    let mut fx = build_app().await;
    {
        let mut cs = fx.app.client_state.write().await;
        cs.playlists.focus = 1;
    }
    fx.app.handle_key(key(KeyCode::Left)).await.unwrap();
    assert_eq!(fx.app.client_state.read().await.playlists.focus, 0);
}

#[tokio::test]
#[serial]
async fn right_with_no_songs_is_noop() {
    let mut fx = build_app().await;
    fx.app.handle_key(key(KeyCode::Right)).await.unwrap();
    assert_eq!(fx.app.client_state.read().await.playlists.focus, 0);
}

#[tokio::test]
#[serial]
async fn right_with_songs_focuses_song_pane_and_initializes() {
    let mut fx = build_app().await;
    {
        let mut cs = fx.app.client_state.write().await;
        cs.playlists.songs = vec![song("s0")];
    }
    fx.app.handle_key(key(KeyCode::Right)).await.unwrap();
    let cs = fx.app.client_state.read().await;
    assert_eq!(cs.playlists.focus, 1);
    assert_eq!(cs.playlists.selected_song, Some(0));
}

#[tokio::test]
#[serial]
async fn up_in_playlist_tree_navigates() {
    let mut fx = build_app().await;
    {
        let mut ds = fx.app.daemon_state.write().await;
        ds.library.playlists = vec![playlist("p0", "A"), playlist("p1", "B")];
    }
    fx.app.handle_key(key(KeyCode::Down)).await.unwrap();
    fx.app.handle_key(key(KeyCode::Down)).await.unwrap();
    fx.app.handle_key(key(KeyCode::Up)).await.unwrap();
    let cs = fx.app.client_state.read().await;
    assert_eq!(cs.playlists.selected_playlist, Some(0));
}

#[tokio::test]
#[serial]
async fn up_at_top_stays_at_zero() {
    let mut fx = build_app().await;
    {
        let mut ds = fx.app.daemon_state.write().await;
        ds.library.playlists = vec![playlist("p0", "A")];
    }
    fx.app.handle_key(key(KeyCode::Down)).await.unwrap();
    fx.app.handle_key(key(KeyCode::Up)).await.unwrap();
    fx.app.handle_key(key(KeyCode::Up)).await.unwrap();
    assert_eq!(
        fx.app.client_state.read().await.playlists.selected_playlist,
        Some(0)
    );
}

#[tokio::test]
#[serial]
async fn k_acts_as_up_in_tree() {
    let mut fx = build_app().await;
    {
        let mut ds = fx.app.daemon_state.write().await;
        ds.library.playlists = vec![playlist("p0", "A"), playlist("p1", "B")];
    }
    fx.app.handle_key(key(KeyCode::Down)).await.unwrap();
    fx.app.handle_key(key(KeyCode::Down)).await.unwrap();
    fx.app.handle_key(key(KeyCode::Char('k'))).await.unwrap();
    assert_eq!(
        fx.app.client_state.read().await.playlists.selected_playlist,
        Some(0)
    );
}

#[tokio::test]
#[serial]
async fn j_acts_as_down_in_tree() {
    let mut fx = build_app().await;
    {
        let mut ds = fx.app.daemon_state.write().await;
        ds.library.playlists = vec![playlist("p0", "A"), playlist("p1", "B")];
    }
    fx.app.handle_key(key(KeyCode::Char('j'))).await.unwrap();
    assert_eq!(
        fx.app.client_state.read().await.playlists.selected_playlist,
        Some(0)
    );
}

#[tokio::test]
#[serial]
async fn down_in_song_pane_initializes_with_no_selection() {
    let mut fx = build_app().await;
    {
        let mut cs = fx.app.client_state.write().await;
        cs.playlists.songs = vec![song("s0"), song("s1")];
        cs.playlists.focus = 1;
    }
    fx.app.handle_key(key(KeyCode::Down)).await.unwrap();
    assert_eq!(
        fx.app.client_state.read().await.playlists.selected_song,
        Some(0)
    );
}

#[tokio::test]
#[serial]
async fn up_in_song_pane_with_selection_decrements() {
    let mut fx = build_app().await;
    {
        let mut cs = fx.app.client_state.write().await;
        cs.playlists.songs = vec![song("s0"), song("s1"), song("s2")];
        cs.playlists.focus = 1;
        cs.playlists.selected_song = Some(2);
    }
    fx.app.handle_key(key(KeyCode::Up)).await.unwrap();
    assert_eq!(
        fx.app.client_state.read().await.playlists.selected_song,
        Some(1)
    );
}

#[tokio::test]
#[serial]
async fn down_in_song_pane_past_max_stays() {
    let mut fx = build_app().await;
    {
        let mut cs = fx.app.client_state.write().await;
        cs.playlists.songs = vec![song("s0"), song("s1")];
        cs.playlists.focus = 1;
        cs.playlists.selected_song = Some(1);
    }
    fx.app.handle_key(key(KeyCode::Down)).await.unwrap();
    assert_eq!(
        fx.app.client_state.read().await.playlists.selected_song,
        Some(1)
    );
}

#[tokio::test]
#[serial]
async fn enter_on_playlist_loads_songs_focus_one() {
    let mut fx = build_app().await;
    {
        let mut ds = fx.app.daemon_state.write().await;
        ds.library.playlists = vec![playlist("p0", "Mix")];
    }
    {
        let mut cs = fx.app.client_state.write().await;
        cs.playlists.selected_playlist = Some(0);
    }
    fx.app.handle_key(key(KeyCode::Enter)).await.unwrap();
    let cs = fx.app.client_state.read().await;
    assert_eq!(cs.playlists.focus, 1);
}

#[tokio::test]
#[serial]
async fn enter_in_song_pane_replays_from_index() {
    let mut fx = build_app().await;
    {
        let mut cs = fx.app.client_state.write().await;
        cs.playlists.focus = 1;
        cs.playlists.songs = vec![song("s0"), song("s1")];
        cs.playlists.selected_song = Some(1);
    }
    fx.app.handle_key(key(KeyCode::Enter)).await.unwrap();
    let ds = fx.app.daemon_state.read().await;
    assert!(ds.queue.iter().any(|s| s.id == "s1"));
}

#[tokio::test]
#[serial]
async fn e_in_song_pane_appends_selected() {
    let mut fx = build_app().await;
    {
        let mut cs = fx.app.client_state.write().await;
        cs.playlists.focus = 1;
        cs.playlists.songs = vec![song("appendme")];
        cs.playlists.selected_song = Some(0);
    }
    fx.app.handle_key(key(KeyCode::Char('e'))).await.unwrap();
    let ds = fx.app.daemon_state.read().await;
    assert!(ds.queue.iter().any(|s| s.id == "appendme"));
}

#[tokio::test]
#[serial]
async fn e_in_tree_pane_appends_all_songs() {
    let mut fx = build_app().await;
    {
        let mut cs = fx.app.client_state.write().await;
        cs.playlists.focus = 0;
        cs.playlists.songs = vec![song("a"), song("b"), song("c")];
    }
    fx.app.handle_key(key(KeyCode::Char('e'))).await.unwrap();
    let ds = fx.app.daemon_state.read().await;
    assert_eq!(ds.queue.len(), 3);
}

#[tokio::test]
#[serial]
async fn i_in_song_pane_with_current_position_inserts_after() {
    let mut fx = build_app().await;
    {
        let mut ds = fx.app.daemon_state.write().await;
        ds.queue.push(song("existing"));
        ds.queue_position = Some(0);
    }
    {
        let mut cs = fx.app.client_state.write().await;
        cs.playlists.focus = 1;
        cs.playlists.songs = vec![song("inserted")];
        cs.playlists.selected_song = Some(0);
    }
    fx.app.handle_key(key(KeyCode::Char('i'))).await.unwrap();
    let ds = fx.app.daemon_state.read().await;
    assert!(ds.queue.iter().any(|s| s.id == "inserted"));
}

#[tokio::test]
#[serial]
async fn i_in_song_pane_without_position_appends() {
    let mut fx = build_app().await;
    {
        let mut cs = fx.app.client_state.write().await;
        cs.playlists.focus = 1;
        cs.playlists.songs = vec![song("inserted")];
        cs.playlists.selected_song = Some(0);
    }
    fx.app.handle_key(key(KeyCode::Char('i'))).await.unwrap();
    let ds = fx.app.daemon_state.read().await;
    assert!(ds.queue.iter().any(|s| s.id == "inserted"));
}

#[tokio::test]
#[serial]
async fn t_shuffles_and_plays_all_songs() {
    let mut fx = build_app().await;
    {
        let mut cs = fx.app.client_state.write().await;
        cs.playlists.songs = vec![song("a"), song("b"), song("c")];
    }
    fx.app.handle_key(key(KeyCode::Char('t'))).await.unwrap();
    let ds = fx.app.daemon_state.read().await;
    assert_eq!(ds.queue.len(), 3);
}

#[tokio::test]
#[serial]
async fn t_with_empty_songs_is_noop() {
    let mut fx = build_app().await;
    fx.app.handle_key(key(KeyCode::Char('t'))).await.unwrap();
    let ds = fx.app.daemon_state.read().await;
    assert!(ds.queue.is_empty());
}

#[tokio::test]
#[serial]
async fn m_in_song_pane_stars_selected_song() {
    let mut fx = build_app().await;
    {
        let mut cs = fx.app.client_state.write().await;
        cs.playlists.focus = 1;
        cs.playlists.songs = vec![song("starme")];
        cs.playlists.selected_song = Some(0);
    }
    fx.app.handle_key(key(KeyCode::Char('m'))).await.unwrap();
    let cs = fx.app.client_state.read().await;
    assert_eq!(cs.playlists.selected_song, Some(0));
    assert_eq!(cs.playlists.focus, 1);
    assert_eq!(cs.playlists.songs.len(), 1);
    assert_eq!(cs.playlists.songs[0].id, "starme");
}

#[tokio::test]
#[serial]
async fn unhandled_key_is_silent() {
    let mut fx = build_app().await;
    let focus_before = fx.app.client_state.read().await.playlists.focus;
    fx.app.handle_key(key(KeyCode::Insert)).await.unwrap();
    let cs = fx.app.client_state.read().await;
    assert_eq!(cs.playlists.focus, focus_before);
    assert!(cs.playlists.songs.is_empty());
    assert!(cs.playlists.selected_song.is_none());
}
