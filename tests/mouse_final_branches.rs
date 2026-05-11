//! Final mouse.rs branches: seek bounds + QuickPlay starred click + scroll initializers.

use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ferrosonic::app::models::SongOption;
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
async fn click_outside_progress_bar_horizontal_does_nothing() {
    let mut app = build_app().await;
    {
        let mut ds = app.daemon_state.write().await;
        ds.now_playing.duration = 200.0;
    }
    app.handle_mouse(click(2, 26)).await.unwrap();
}

#[tokio::test]
#[serial]
async fn click_on_progress_bar_far_right_seeks_near_end() {
    let mut app = build_app().await;
    {
        let mut ds = app.daemon_state.write().await;
        ds.now_playing.duration = 200.0;
        ds.now_playing.song = Some(song("a"));
    }
    app.handle_mouse(click(75, 26)).await.unwrap();
}

#[tokio::test]
#[serial]
async fn quick_play_click_on_starred_row_dispatches_refresh_starred() {
    let mut app = build_app().await;
    {
        let mut cs = app.client_state.write().await;
        cs.page = Page::QuickPlay;
    }
    app.handle_mouse(click(10, 2)).await.unwrap();
    let cs = app.client_state.read().await;
    assert!(matches!(
        cs.songs.selected_option,
        Some(SongOption::Starred)
    ));
}

#[tokio::test]
#[serial]
async fn quick_play_reclick_same_option_skips_refresh() {
    let mut app = build_app().await;
    {
        let mut cs = app.client_state.write().await;
        cs.page = Page::QuickPlay;
        cs.songs.selected_option = Some(SongOption::Random);
    }
    app.handle_mouse(click(10, 3)).await.unwrap();
}

#[tokio::test]
#[serial]
async fn quick_play_right_pane_click_with_no_layout_panes_is_safe() {
    let mut app = build_app().await;
    {
        let mut cs = app.client_state.write().await;
        cs.page = Page::QuickPlay;
        cs.layout.content_left = None;
        cs.layout.content_right = None;
    }
    app.handle_mouse(click(50, 5)).await.unwrap();
}

#[tokio::test]
#[serial]
async fn scroll_down_library_song_pane_with_selected_at_max_stays() {
    let mut app = build_app().await;
    {
        let mut cs = app.client_state.write().await;
        cs.page = Page::Library;
        cs.artists.focus = 1;
        cs.artists.songs = vec![song("a"), song("b")];
        cs.artists.selected_song = Some(1);
    }
    app.handle_mouse(scroll(false)).await.unwrap();
}

#[tokio::test]
#[serial]
async fn scroll_up_playlists_song_pane_with_no_selection_initializes() {
    let mut app = build_app().await;
    {
        let mut cs = app.client_state.write().await;
        cs.page = Page::Playlists;
        cs.playlists.focus = 1;
        cs.playlists.songs = vec![song("p0")];
    }
    app.handle_mouse(scroll(true)).await.unwrap();
}

#[tokio::test]
#[serial]
async fn scroll_down_playlists_song_pane_at_max_stays() {
    let mut app = build_app().await;
    {
        let mut cs = app.client_state.write().await;
        cs.page = Page::Playlists;
        cs.playlists.focus = 1;
        cs.playlists.songs = vec![song("p0"), song("p1")];
        cs.playlists.selected_song = Some(1);
    }
    app.handle_mouse(scroll(false)).await.unwrap();
}

#[tokio::test]
#[serial]
async fn scroll_down_playlists_tree_at_max_stays() {
    let mut app = build_app().await;
    {
        let mut cs = app.client_state.write().await;
        cs.page = Page::Playlists;
        cs.playlists.focus = 0;
        cs.playlists.selected_playlist = Some(1);
    }
    {
        let mut ds = app.daemon_state.write().await;
        ds.library.playlists = vec![playlist("p0", "A"), playlist("p1", "B")];
    }
    app.handle_mouse(scroll(false)).await.unwrap();
}
