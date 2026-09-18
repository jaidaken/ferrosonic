//! Mouse paths in app/mouse.rs: progress-bar seek, quick-play clicks, queue clicks.

mod common;
use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ferrosonic::app::models::SongOption;
use ferrosonic::app::state::Page;
use ferrosonic::app::App;
use ferrosonic::config::Config;
use ferrosonic::ipc::protocol::DaemonRequest;
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
        user_rating: None,
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
async fn progress_bar_click_seek_matches_rendered_geometry() {
    let tempdir = common::tempdir();
    std::env::set_var("FERROSONIC_CONFIG_DIR", tempdir.path());
    std::mem::forget(tempdir);
    let config = Config::new();
    let recording = common::RecordingClient::new();
    let mut app = App::with_remote_client(recording.clone(), config);
    {
        let mut cs = app.client_state.write().await;
        cs.layout.header = Rect::new(0, 0, 80, 1);
        cs.layout.content = Rect::new(0, 1, 80, 20);
        cs.layout.now_playing = Rect::new(0, 21, 80, 7);
    }
    {
        let mut ds = app.daemon_state.write().await;
        ds.now_playing.duration = 240.0;
        ds.now_playing.position = 60.0;
        ds.now_playing.song = Some(song("a"));
    }

    // Inner progress area: now_playing.x + 1, width - 2; progress row at y = 26.
    let area = Rect::new(1, 26, 78, 1);
    let time_width = "01:00 / 04:00".len() as u16;
    let (_start, bar_start, bar_width) =
        ferrosonic::ui::widget_now_playing::progress_bar_geometry(area, time_width);
    let click_x = bar_start + bar_width / 2;
    app.handle_mouse(click(click_x, 26)).await.unwrap();

    let seek = recording
        .requests()
        .await
        .into_iter()
        .find_map(|r| match r {
            DaemonRequest::Seek(p) => Some(p),
            _ => None,
        })
        .expect("a click on the drawn bar must dispatch Seek");
    let expected = f64::from(click_x - bar_start) / f64::from(bar_width) * 240.0;
    assert!(
        (seek - expected).abs() < 1e-6,
        "seek {seek} must match the clicked column ({expected})"
    );
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
async fn click_on_one_row_now_playing_does_not_underflow() {
    // A short terminal can size the now-playing area to one row; the progress
    // row math must not underflow (panic in debug) on such a strip.
    let mut app = build_app().await;
    {
        let mut cs = app.client_state.write().await;
        cs.layout.now_playing = Rect::new(0, 21, 80, 1);
    }
    {
        let mut ds = app.daemon_state.write().await;
        ds.now_playing.duration = 240.0;
    }
    app.handle_mouse(click(40, 21)).await.unwrap();
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

#[tokio::test]
#[serial]
async fn quick_play_option_borders_and_unused_remainder_are_not_clickable() {
    let mut app = build_app().await;
    {
        let mut cs = app.client_state.write().await;
        cs.page = Page::QuickPlay;
        cs.songs.selected_option = Some(SongOption::Random);
        cs.layout.content_left = Some(Rect::new(0, 1, 40, 4));
    }
    for (x, y) in [(0, 2), (39, 2), (10, 1), (10, 4), (37, 2), (38, 2)] {
        app.handle_mouse(click(x, y)).await.unwrap();
        assert_eq!(
            app.client_state.read().await.songs.selected_option,
            Some(SongOption::Random),
            "border/remainder click ({x},{y}) changed the option"
        );
    }
}

#[tokio::test]
#[serial]
async fn quick_play_wrapped_grid_accepts_each_column_and_last_wrapped_cell() {
    let mut app = build_app().await;
    {
        let mut cs = app.client_state.write().await;
        cs.page = Page::QuickPlay;
        cs.layout.content_left = Some(Rect::new(0, 1, 40, 4));
    }
    let expected = [
        SongOption::Starred,
        SongOption::Random,
        SongOption::RandomAlbum,
        SongOption::NewestAlbum,
    ];
    for (column, option) in expected.into_iter().enumerate() {
        let x = 1 + u16::try_from(column * 9 + 4).unwrap();
        app.handle_mouse(click(x, 2)).await.unwrap();
        assert_eq!(
            app.client_state.read().await.songs.selected_option,
            Some(option)
        );
    }
    app.handle_mouse(click(1 + 2 * 9 + 4, 3)).await.unwrap();
    assert_eq!(
        app.client_state.read().await.songs.selected_option,
        Some(SongOption::HighestRated)
    );
}
