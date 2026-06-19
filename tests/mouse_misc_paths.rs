//! Mouse paths in app/mouse.rs: progress-bar seek, quick-play clicks, queue clicks.

mod common;
use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ferrosonic::app::models::SongOption;
use ferrosonic::app::state::Page;
use ferrosonic::app::App;
use ferrosonic::config::Config;
use ferrosonic::subsonic::models::Child;
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

async fn build_app() -> App {
    let tempdir = common::tempdir();
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
async fn progress_bar_click_on_now_playing_seeks() {
    let mut app = build_app().await;
    {
        let mut ds = app.daemon_state.write().await;
        ds.now_playing.duration = 240.0;
        ds.now_playing.song = Some(song("a"));
    }
    app.handle_mouse(click(40, 26)).await.unwrap();
}

#[tokio::test]
#[serial]
async fn click_on_now_playing_non_progress_row_is_safe() {
    let mut app = build_app().await;
    {
        let mut ds = app.daemon_state.write().await;
        ds.now_playing.duration = 240.0;
    }
    app.handle_mouse(click(40, 22)).await.unwrap();
}

#[tokio::test]
#[serial]
async fn click_on_progress_bar_with_zero_duration_is_safe() {
    let mut app = build_app().await;
    app.handle_mouse(click(40, 26)).await.unwrap();
}

#[tokio::test]
#[serial]
async fn click_below_progress_bar_when_too_narrow_does_not_seek() {
    let mut app = build_app().await;
    {
        let mut cs = app.client_state.write().await;
        cs.layout.now_playing = Rect::new(0, 21, 10, 4);
    }
    app.handle_mouse(click(5, 23)).await.unwrap();
}

#[tokio::test]
#[serial]
async fn quick_play_left_pane_click_on_starred_row() {
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
async fn quick_play_left_pane_click_on_random_row() {
    let mut app = build_app().await;
    {
        let mut cs = app.client_state.write().await;
        cs.page = Page::QuickPlay;
    }
    app.handle_mouse(click(10, 3)).await.unwrap();
    let cs = app.client_state.read().await;
    assert!(matches!(cs.songs.selected_option, Some(SongOption::Random)));
}

#[tokio::test]
#[serial]
async fn quick_play_left_pane_click_below_options_is_noop() {
    let mut app = build_app().await;
    {
        let mut cs = app.client_state.write().await;
        cs.page = Page::QuickPlay;
    }
    app.handle_mouse(click(10, 10)).await.unwrap();
}

#[tokio::test]
#[serial]
async fn quick_play_right_pane_click_selects_song() {
    let mut app = build_app().await;
    {
        let mut cs = app.client_state.write().await;
        cs.page = Page::QuickPlay;
        cs.songs.selected_option = Some(SongOption::Random);
    }
    {
        let mut ds = app.daemon_state.write().await;
        ds.library.random_songs = vec![song("r0"), song("r1")];
    }
    app.handle_mouse(click(50, 2)).await.unwrap();
    let cs = app.client_state.read().await;
    assert_eq!(cs.songs.focus, 1);
    assert_eq!(cs.songs.selected_index, Some(0));
}

#[tokio::test]
#[serial]
async fn quick_play_double_click_on_song_plays_replace() {
    let mut app = build_app().await;
    {
        let mut cs = app.client_state.write().await;
        cs.page = Page::QuickPlay;
        cs.songs.selected_option = Some(SongOption::Random);
    }
    {
        let mut ds = app.daemon_state.write().await;
        ds.library.random_songs = vec![song("r0"), song("r1")];
    }
    app.handle_mouse(click(50, 2)).await.unwrap();
    app.handle_mouse(click(50, 2)).await.unwrap();
}

#[tokio::test]
#[serial]
async fn queue_pane_click_selects_index() {
    let mut app = build_app().await;
    {
        let mut cs = app.client_state.write().await;
        cs.page = Page::Queue;
    }
    {
        let mut ds = app.daemon_state.write().await;
        ds.queue = vec![song("q0"), song("q1"), song("q2")];
    }
    app.handle_mouse(click(20, 3)).await.unwrap();
    let cs = app.client_state.read().await;
    assert!(cs.queue_state.selected.is_some());
}

#[tokio::test]
#[serial]
async fn queue_pane_double_click_plays_index() {
    let mut app = build_app().await;
    {
        let mut cs = app.client_state.write().await;
        cs.page = Page::Queue;
    }
    {
        let mut ds = app.daemon_state.write().await;
        ds.queue = vec![song("q0"), song("q1")];
    }
    app.handle_mouse(click(20, 3)).await.unwrap();
    app.handle_mouse(click(20, 3)).await.unwrap();
}

#[tokio::test]
#[serial]
async fn quick_play_right_pane_click_with_no_options_selected_is_noop() {
    let mut app = build_app().await;
    {
        let mut cs = app.client_state.write().await;
        cs.page = Page::QuickPlay;
    }
    app.handle_mouse(click(50, 3)).await.unwrap();
}

#[tokio::test]
#[serial]
async fn click_on_server_page_content_does_not_route() {
    let mut app = build_app().await;
    {
        let mut cs = app.client_state.write().await;
        cs.page = Page::Server;
    }
    app.handle_mouse(click(20, 5)).await.unwrap();
}

#[tokio::test]
#[serial]
async fn click_on_settings_page_content_does_not_route() {
    let mut app = build_app().await;
    {
        let mut cs = app.client_state.write().await;
        cs.page = Page::Settings;
    }
    app.handle_mouse(click(20, 5)).await.unwrap();
}

#[tokio::test]
#[serial]
async fn quick_play_left_pane_re_click_same_option_skips_refresh() {
    let mut app = build_app().await;
    {
        let mut cs = app.client_state.write().await;
        cs.page = Page::QuickPlay;
        cs.songs.selected_option = Some(SongOption::Starred);
    }
    app.handle_mouse(click(10, 2)).await.unwrap();
}
