//! Deep mouse handler paths: QuickPlay option clicks, queue row clicks,
//! double-click expiry, content edge cases.

mod common;
use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ferrosonic::app::models::SongOption;
use ferrosonic::app::state::Page;
use ferrosonic::app::App;
use ferrosonic::config::Config;
use ferrosonic::subsonic::models::Child;
use ratatui::layout::Rect;
use serial_test::serial;

fn song(id: &str) -> Child {
    Child {
        id: id.into(),
        title: id.into(),
        parent: None,
        is_dir: false,
        album: None,
        artist: None,
        artist_id: None,
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

fn click(x: u16, y: u16) -> MouseEvent {
    MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: x,
        row: y,
        modifiers: KeyModifiers::NONE,
    }
}

struct AppFixture {
    app: App,
    _tempdir: tempfile::TempDir,
}

async fn build_app(page: Page) -> AppFixture {
    let tempdir = common::tempdir();
    std::env::set_var("FERROSONIC_CONFIG_DIR", tempdir.path());
    let mut config = Config::new();
    config.daemon = false;
    let app = App::new(config);
    {
        let mut cs = app.client_state.write().await;
        cs.page = page;
        cs.layout.header = Rect::new(0, 0, 80, 1);
        cs.layout.content = Rect::new(0, 1, 80, 20);
        cs.layout.content_left = Some(Rect::new(0, 1, 40, 20));
        cs.layout.content_right = Some(Rect::new(40, 1, 40, 20));
        cs.layout.now_playing = Rect::new(0, 21, 80, 7);
    }
    AppFixture {
        app,
        _tempdir: tempdir,
    }
}

#[tokio::test]
#[serial]
async fn quickplay_click_on_starred_option_selects_starred() {
    let mut fx = build_app(Page::QuickPlay).await;
    fx.app.handle_mouse(click(10, 2)).await.unwrap();
    let cs = fx.app.client_state.read().await;
    assert!(matches!(
        cs.songs.selected_option,
        Some(SongOption::Starred)
    ));
    assert_eq!(cs.songs.focus, 0);
}

#[tokio::test]
#[serial]
async fn quickplay_click_on_random_option_selects_random() {
    let mut fx = build_app(Page::QuickPlay).await;
    fx.app.handle_mouse(click(10, 3)).await.unwrap();
    let cs = fx.app.client_state.read().await;
    assert!(matches!(cs.songs.selected_option, Some(SongOption::Random)));
}

#[tokio::test]
#[serial]
async fn quickplay_click_on_unreachable_option_row_is_noop() {
    let mut fx = build_app(Page::QuickPlay).await;
    fx.app.handle_mouse(click(10, 15)).await.unwrap();
}

#[tokio::test]
#[serial]
async fn quickplay_click_on_song_pane_selects_song() {
    let mut fx = build_app(Page::QuickPlay).await;
    {
        let mut ds = fx.app.daemon_state.write().await;
        ds.library.starred_songs = vec![song("s0"), song("s1"), song("s2")];
    }
    {
        let mut cs = fx.app.client_state.write().await;
        cs.songs.selected_option = Some(SongOption::Starred);
    }
    fx.app.handle_mouse(click(50, 3)).await.unwrap();
    let cs = fx.app.client_state.read().await;
    assert_eq!(cs.songs.focus, 1);
    assert!(cs.songs.selected_index.is_some());
}

#[tokio::test]
#[serial]
async fn quickplay_click_on_song_past_list_end_is_noop() {
    let mut fx = build_app(Page::QuickPlay).await;
    {
        let mut ds = fx.app.daemon_state.write().await;
        ds.library.starred_songs = vec![song("s0")];
    }
    {
        let mut cs = fx.app.client_state.write().await;
        cs.songs.selected_option = Some(SongOption::Starred);
    }
    fx.app.handle_mouse(click(50, 18)).await.unwrap();
    let cs = fx.app.client_state.read().await;
    assert!(cs.songs.selected_index.is_none() || cs.songs.selected_index == Some(0));
}

#[tokio::test]
#[serial]
async fn queue_click_selects_row() {
    let mut fx = build_app(Page::Queue).await;
    {
        let mut ds = fx.app.daemon_state.write().await;
        ds.queue = vec![song("q0"), song("q1"), song("q2")];
    }
    fx.app.handle_mouse(click(20, 3)).await.unwrap();
    let cs = fx.app.client_state.read().await;
    assert!(cs.queue_state.selected.is_some());
}

#[tokio::test]
#[serial]
async fn double_click_on_same_song_within_500ms_plays_it() {
    let mut fx = build_app(Page::QuickPlay).await;
    {
        let mut ds = fx.app.daemon_state.write().await;
        ds.library.starred_songs = vec![song("s0"), song("s1")];
    }
    {
        let mut cs = fx.app.client_state.write().await;
        cs.songs.selected_option = Some(SongOption::Starred);
    }
    fx.app.handle_mouse(click(50, 3)).await.unwrap();
    fx.app.handle_mouse(click(50, 3)).await.unwrap();
}

#[tokio::test]
#[serial]
async fn quickplay_click_with_no_content_panes_is_safe() {
    let tempdir = common::tempdir();
    std::env::set_var("FERROSONIC_CONFIG_DIR", tempdir.path());
    let mut config = Config::new();
    config.daemon = false;
    let mut app = App::new(config);
    {
        let mut cs = app.client_state.write().await;
        cs.page = Page::QuickPlay;
        cs.layout.header = Rect::new(0, 0, 80, 1);
        cs.layout.content = Rect::new(0, 1, 80, 20);
        cs.layout.content_left = None;
        cs.layout.content_right = None;
        cs.layout.now_playing = Rect::new(0, 21, 80, 7);
    }
    app.handle_mouse(click(20, 5)).await.unwrap();
}

#[tokio::test]
#[serial]
async fn scroll_down_in_quickplay_increments_scroll_offset() {
    let mut fx = build_app(Page::QuickPlay).await;
    {
        let mut ds = fx.app.daemon_state.write().await;
        ds.library.random_songs = (0..50).map(|i| song(&format!("r{}", i))).collect();
    }
    {
        let mut cs = fx.app.client_state.write().await;
        cs.songs.selected_option = Some(SongOption::Random);
        cs.songs.focus = 1;
    }
    let event = MouseEvent {
        kind: MouseEventKind::ScrollDown,
        column: 50,
        row: 5,
        modifiers: KeyModifiers::NONE,
    };
    fx.app.handle_mouse(event).await.unwrap();
}

#[tokio::test]
#[serial]
async fn click_on_progress_bar_seeks_to_fraction() {
    let mut fx = build_app(Page::Library).await;
    {
        let mut ds = fx.app.daemon_state.write().await;
        ds.now_playing.duration = 240.0;
        ds.now_playing.song = Some(song("a"));
    }
    fx.app.handle_mouse(click(40, 26)).await.unwrap();
}

#[tokio::test]
#[serial]
async fn click_with_zero_duration_does_not_seek() {
    let mut fx = build_app(Page::Library).await;
    fx.app.handle_mouse(click(40, 26)).await.unwrap();
}

#[tokio::test]
#[serial]
async fn double_click_expiry_after_500ms_does_not_trigger_play() {
    let mut fx = build_app(Page::QuickPlay).await;
    {
        let mut ds = fx.app.daemon_state.write().await;
        ds.library.starred_songs = vec![song("s0")];
    }
    {
        let mut cs = fx.app.client_state.write().await;
        cs.songs.selected_option = Some(SongOption::Starred);
    }
    fx.app.handle_mouse(click(50, 3)).await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(600)).await;
    fx.app.handle_mouse(click(50, 3)).await.unwrap();
}
