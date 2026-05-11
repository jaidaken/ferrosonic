//! Exhaustive mouse-scroll branches: every page, both directions, focus paths.

use crossterm::event::{KeyModifiers, MouseEvent, MouseEventKind};
use ferrosonic::app::state::Page;
use ferrosonic::app::App;
use ferrosonic::config::Config;
use ferrosonic::subsonic::models::{Artist, Child, Playlist};
use serial_test::serial;

fn scroll(up: bool) -> MouseEvent {
    MouseEvent {
        kind: if up {
            MouseEventKind::ScrollUp
        } else {
            MouseEventKind::ScrollDown
        },
        column: 0,
        row: 0,
        modifiers: KeyModifiers::NONE,
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

fn artist(id: &str, name: &str) -> Artist {
    Artist {
        id: id.into(),
        name: name.into(),
        album_count: Some(1),
        cover_art: None,
    }
}

fn playlist(id: &str, name: &str) -> Playlist {
    Playlist {
        id: id.into(),
        name: name.into(),
        owner: None,
        song_count: Some(1),
        duration: Some(60),
        cover_art: None,
        public: None,
        comment: None,
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
    let app = App::new(config);
    AppFixture {
        app,
        _tempdir: tempdir,
    }
}

#[tokio::test]
#[serial]
async fn scroll_up_on_library_tree_decrements_selection() {
    let mut fx = build_app().await;
    {
        let mut cs = fx.app.client_state.write().await;
        cs.page = Page::Library;
        cs.artists.selected_index = Some(2);
    }
    fx.app.handle_mouse(scroll(true)).await.unwrap();
    assert_eq!(
        fx.app.client_state.read().await.artists.selected_index,
        Some(1)
    );
}

#[tokio::test]
#[serial]
async fn scroll_down_on_library_tree_increments_selection() {
    let mut fx = build_app().await;
    {
        let mut ds = fx.app.daemon_state.write().await;
        ds.library.artists = vec![artist("a0", "A"), artist("a1", "B"), artist("a2", "C")];
    }
    {
        let mut cs = fx.app.client_state.write().await;
        cs.page = Page::Library;
        cs.artists.selected_index = Some(0);
    }
    fx.app.handle_mouse(scroll(false)).await.unwrap();
    assert_eq!(
        fx.app.client_state.read().await.artists.selected_index,
        Some(1)
    );
}

#[tokio::test]
#[serial]
async fn scroll_up_on_library_song_pane_decrements() {
    let mut fx = build_app().await;
    {
        let mut cs = fx.app.client_state.write().await;
        cs.page = Page::Library;
        cs.artists.focus = 1;
        cs.artists.selected_song = Some(2);
    }
    fx.app.handle_mouse(scroll(true)).await.unwrap();
    assert_eq!(
        fx.app.client_state.read().await.artists.selected_song,
        Some(1)
    );
}

#[tokio::test]
#[serial]
async fn scroll_down_on_library_song_pane_increments() {
    let mut fx = build_app().await;
    {
        let mut cs = fx.app.client_state.write().await;
        cs.page = Page::Library;
        cs.artists.focus = 1;
        cs.artists.songs = vec![song("a"), song("b")];
        cs.artists.selected_song = Some(0);
    }
    fx.app.handle_mouse(scroll(false)).await.unwrap();
    assert_eq!(
        fx.app.client_state.read().await.artists.selected_song,
        Some(1)
    );
}

#[tokio::test]
#[serial]
async fn scroll_down_on_empty_library_initializes_none() {
    let mut fx = build_app().await;
    {
        let mut cs = fx.app.client_state.write().await;
        cs.page = Page::Library;
    }
    fx.app.handle_mouse(scroll(false)).await.unwrap();
    assert!(fx
        .app
        .client_state
        .read()
        .await
        .artists
        .selected_index
        .is_none());
}

#[tokio::test]
#[serial]
async fn scroll_up_on_queue_decrements_selection() {
    let mut fx = build_app().await;
    {
        let mut ds = fx.app.daemon_state.write().await;
        ds.queue = vec![song("a"), song("b"), song("c")];
    }
    {
        let mut cs = fx.app.client_state.write().await;
        cs.page = Page::Queue;
        cs.queue_state.selected = Some(2);
    }
    fx.app.handle_mouse(scroll(true)).await.unwrap();
    assert_eq!(
        fx.app.client_state.read().await.queue_state.selected,
        Some(1)
    );
}

#[tokio::test]
#[serial]
async fn scroll_up_on_queue_with_no_selection_initializes_to_zero() {
    let mut fx = build_app().await;
    {
        let mut ds = fx.app.daemon_state.write().await;
        ds.queue = vec![song("a")];
    }
    {
        let mut cs = fx.app.client_state.write().await;
        cs.page = Page::Queue;
    }
    fx.app.handle_mouse(scroll(true)).await.unwrap();
    assert_eq!(
        fx.app.client_state.read().await.queue_state.selected,
        Some(0)
    );
}

#[tokio::test]
#[serial]
async fn scroll_down_on_queue_increments() {
    let mut fx = build_app().await;
    {
        let mut ds = fx.app.daemon_state.write().await;
        ds.queue = vec![song("a"), song("b")];
    }
    {
        let mut cs = fx.app.client_state.write().await;
        cs.page = Page::Queue;
        cs.queue_state.selected = Some(0);
    }
    fx.app.handle_mouse(scroll(false)).await.unwrap();
    assert_eq!(
        fx.app.client_state.read().await.queue_state.selected,
        Some(1)
    );
}

#[tokio::test]
#[serial]
async fn scroll_up_on_quickplay_song_pane_decrements() {
    let mut fx = build_app().await;
    {
        let mut ds = fx.app.daemon_state.write().await;
        ds.library.random_songs = vec![song("a"), song("b")];
    }
    {
        let mut cs = fx.app.client_state.write().await;
        cs.page = Page::QuickPlay;
        cs.songs.focus = 1;
        cs.songs.selected_option = Some(ferrosonic::app::models::SongOption::Random);
        cs.songs.selected_index = Some(1);
    }
    fx.app.handle_mouse(scroll(true)).await.unwrap();
    assert_eq!(
        fx.app.client_state.read().await.songs.selected_index,
        Some(0)
    );
}

#[tokio::test]
#[serial]
async fn scroll_down_on_quickplay_song_pane_increments() {
    let mut fx = build_app().await;
    {
        let mut ds = fx.app.daemon_state.write().await;
        ds.library.random_songs = vec![song("a"), song("b"), song("c")];
    }
    {
        let mut cs = fx.app.client_state.write().await;
        cs.page = Page::QuickPlay;
        cs.songs.focus = 1;
        cs.songs.selected_option = Some(ferrosonic::app::models::SongOption::Random);
        cs.songs.selected_index = Some(0);
    }
    fx.app.handle_mouse(scroll(false)).await.unwrap();
    assert_eq!(
        fx.app.client_state.read().await.songs.selected_index,
        Some(1)
    );
}

#[tokio::test]
#[serial]
async fn scroll_on_playlists_tree_navigates() {
    let mut fx = build_app().await;
    {
        let mut ds = fx.app.daemon_state.write().await;
        ds.library.playlists = vec![playlist("p0", "A"), playlist("p1", "B")];
    }
    {
        let mut cs = fx.app.client_state.write().await;
        cs.page = Page::Playlists;
        cs.playlists.selected_playlist = Some(1);
    }
    fx.app.handle_mouse(scroll(true)).await.unwrap();
    assert_eq!(
        fx.app.client_state.read().await.playlists.selected_playlist,
        Some(0)
    );
}

#[tokio::test]
#[serial]
async fn scroll_down_on_playlists_tree_increments() {
    let mut fx = build_app().await;
    {
        let mut ds = fx.app.daemon_state.write().await;
        ds.library.playlists = vec![playlist("p0", "A"), playlist("p1", "B")];
    }
    {
        let mut cs = fx.app.client_state.write().await;
        cs.page = Page::Playlists;
        cs.playlists.selected_playlist = Some(0);
    }
    fx.app.handle_mouse(scroll(false)).await.unwrap();
    assert_eq!(
        fx.app.client_state.read().await.playlists.selected_playlist,
        Some(1)
    );
}

#[tokio::test]
#[serial]
async fn scroll_up_on_playlists_song_pane_decrements() {
    let mut fx = build_app().await;
    {
        let mut cs = fx.app.client_state.write().await;
        cs.page = Page::Playlists;
        cs.playlists.focus = 1;
        cs.playlists.songs = vec![song("a"), song("b")];
        cs.playlists.selected_song = Some(1);
    }
    fx.app.handle_mouse(scroll(true)).await.unwrap();
    assert_eq!(
        fx.app.client_state.read().await.playlists.selected_song,
        Some(0)
    );
}

#[tokio::test]
#[serial]
async fn scroll_down_on_playlists_song_pane_increments() {
    let mut fx = build_app().await;
    {
        let mut cs = fx.app.client_state.write().await;
        cs.page = Page::Playlists;
        cs.playlists.focus = 1;
        cs.playlists.songs = vec![song("a"), song("b")];
        cs.playlists.selected_song = Some(0);
    }
    fx.app.handle_mouse(scroll(false)).await.unwrap();
    assert_eq!(
        fx.app.client_state.read().await.playlists.selected_song,
        Some(1)
    );
}

#[tokio::test]
#[serial]
async fn scroll_on_server_page_is_noop() {
    let mut fx = build_app().await;
    {
        let mut cs = fx.app.client_state.write().await;
        cs.page = Page::Server;
    }
    fx.app.handle_mouse(scroll(true)).await.unwrap();
    fx.app.handle_mouse(scroll(false)).await.unwrap();
}

#[tokio::test]
#[serial]
async fn scroll_on_settings_page_is_noop() {
    let mut fx = build_app().await;
    {
        let mut cs = fx.app.client_state.write().await;
        cs.page = Page::Settings;
    }
    fx.app.handle_mouse(scroll(true)).await.unwrap();
    fx.app.handle_mouse(scroll(false)).await.unwrap();
}

#[tokio::test]
#[serial]
async fn unhandled_mouse_kind_is_noop() {
    let mut fx = build_app().await;
    let evt = MouseEvent {
        kind: MouseEventKind::Up(crossterm::event::MouseButton::Left),
        column: 5,
        row: 5,
        modifiers: KeyModifiers::NONE,
    };
    fx.app.handle_mouse(evt).await.unwrap();
}
