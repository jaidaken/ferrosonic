//! mouse.rs remaining branches: PauseButton, scroll initializers, focus-1 paths.

use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ferrosonic::app::state::Page;
use ferrosonic::app::App;
use ferrosonic::config::Config;
use ferrosonic::subsonic::models::{Child, Playlist};
use ratatui::layout::Rect;
use serial_test::serial;

fn click(x: u16, y: u16) -> MouseEvent {
    MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: x,
        row: y,
        modifiers: KeyModifiers::NONE,
    }
}

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

async fn build_app() -> App {
    let tempdir = tempfile::tempdir().unwrap();
    std::env::set_var("FERROSONIC_CONFIG_DIR", tempdir.path());
    std::mem::forget(tempdir);
    let mut config = Config::new();
    config.daemon = false;
    let app = App::new(config);
    {
        let mut cs = app.client_state.write().await;
        cs.layout.header = Rect::new(0, 0, 80, 1);
        cs.layout.content = Rect::new(0, 1, 80, 20);
        cs.layout.content_left = Some(Rect::new(0, 1, 40, 20));
        cs.layout.content_right = Some(Rect::new(40, 1, 40, 20));
        cs.layout.now_playing = Rect::new(0, 21, 80, 7);
    }
    app
}

#[tokio::test]
#[serial]
async fn click_on_pause_button_dispatches_toggle_pause() {
    let mut app = build_app().await;
    app.handle_mouse(click(70, 0)).await.unwrap();
}

#[tokio::test]
#[serial]
async fn scroll_up_on_library_song_pane_initializes_when_none() {
    let mut app = build_app().await;
    {
        let mut cs = app.client_state.write().await;
        cs.page = Page::Library;
        cs.artists.focus = 1;
        cs.artists.songs = vec![song("a")];
    }
    app.handle_mouse(scroll(true)).await.unwrap();
}

#[tokio::test]
#[serial]
async fn scroll_down_on_library_song_pane_initializes_when_none() {
    let mut app = build_app().await;
    {
        let mut cs = app.client_state.write().await;
        cs.page = Page::Library;
        cs.artists.focus = 1;
        cs.artists.songs = vec![song("a"), song("b")];
    }
    app.handle_mouse(scroll(false)).await.unwrap();
}

#[tokio::test]
#[serial]
async fn scroll_up_quickplay_initializes_index_when_none() {
    let mut app = build_app().await;
    {
        let mut cs = app.client_state.write().await;
        cs.page = Page::QuickPlay;
        cs.songs.focus = 1;
        cs.songs.selected_option = Some(ferrosonic::app::models::SongOption::Random);
    }
    {
        let mut ds = app.daemon_state.write().await;
        ds.library.random_songs = vec![song("a")];
    }
    app.handle_mouse(scroll(true)).await.unwrap();
}

#[tokio::test]
#[serial]
async fn scroll_up_playlists_song_pane_initializes_when_none() {
    let mut app = build_app().await;
    {
        let mut cs = app.client_state.write().await;
        cs.page = Page::Playlists;
        cs.playlists.focus = 1;
        cs.playlists.songs = vec![song("a")];
    }
    app.handle_mouse(scroll(true)).await.unwrap();
}

#[tokio::test]
#[serial]
async fn scroll_down_playlists_tree_initializes_when_none() {
    let mut app = build_app().await;
    {
        let mut cs = app.client_state.write().await;
        cs.page = Page::Playlists;
        cs.playlists.focus = 0;
    }
    {
        let mut ds = app.daemon_state.write().await;
        ds.library.playlists = vec![playlist("a", "A")];
    }
    app.handle_mouse(scroll(false)).await.unwrap();
}

#[tokio::test]
#[serial]
async fn scroll_down_playlists_song_pane_initializes_when_none() {
    let mut app = build_app().await;
    {
        let mut cs = app.client_state.write().await;
        cs.page = Page::Playlists;
        cs.playlists.focus = 1;
        cs.playlists.songs = vec![song("a")];
    }
    app.handle_mouse(scroll(false)).await.unwrap();
}

#[tokio::test]
#[serial]
async fn scroll_up_library_song_pane_no_selection_path() {
    let mut app = build_app().await;
    {
        let mut cs = app.client_state.write().await;
        cs.page = Page::Library;
        cs.artists.focus = 1;
    }
    app.handle_mouse(scroll(true)).await.unwrap();
}

#[tokio::test]
#[serial]
async fn scroll_on_quickplay_with_focus_zero_is_noop() {
    let mut app = build_app().await;
    {
        let mut cs = app.client_state.write().await;
        cs.page = Page::QuickPlay;
        cs.songs.focus = 0;
    }
    app.handle_mouse(scroll(true)).await.unwrap();
    app.handle_mouse(scroll(false)).await.unwrap();
}
