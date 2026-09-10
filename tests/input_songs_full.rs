//! Exhaustive input_songs.rs branches (Quick Play page).

mod common;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ferrosonic::app::models::SongOption;
use ferrosonic::app::App;
use ferrosonic::config::Config;
use ferrosonic::ipc::protocol::QuickPlayAlbumKind;
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
async fn down_in_option_pane_moves_a_full_grid_row_on_a_short_pane() {
    let mut fx = build_app().await;
    {
        let mut cs = fx.app.client_state.write().await;
        cs.songs.focus = 0;
        cs.songs.selected_option = Some(SongOption::Starred);
        // height 6 -> 4 rows -> 2 columns, so Starred(row0,col0) is directly
        // above RandomAlbum(row1,col0); Down must move by the column count.
        cs.layout.content_left = Some(ratatui::layout::Rect::new(0, 0, 50, 6));
    }
    fx.app.handle_key(key(KeyCode::Down)).await.unwrap();
    assert_eq!(
        fx.app.client_state.read().await.songs.selected_option,
        Some(SongOption::RandomAlbum),
        "Down must step a full grid row when the pane renders multiple columns"
    );
    fx.app.handle_key(key(KeyCode::Up)).await.unwrap();
    assert_eq!(
        fx.app.client_state.read().await.songs.selected_option,
        Some(SongOption::Starred),
        "Up must step a full grid row back"
    );
}

#[tokio::test]
#[serial]
async fn down_at_random_triggers_random_album_refresh() {
    let mut fx = build_app().await;
    {
        let mut cs = fx.app.client_state.write().await;
        cs.songs.focus = 0;
        cs.songs.selected_option = Some(SongOption::Random);
    }
    fx.app.handle_key(key(KeyCode::Down)).await.unwrap();
    assert!(matches!(
        fx.app.client_state.read().await.songs.selected_option,
        Some(SongOption::RandomAlbum)
    ));
}

#[tokio::test]
#[serial]
async fn up_at_random_album_triggers_random_refresh() {
    let mut fx = build_app().await;
    {
        let mut cs = fx.app.client_state.write().await;
        cs.songs.focus = 0;
        cs.songs.selected_option = Some(SongOption::RandomAlbum);
    }
    fx.app.handle_key(key(KeyCode::Up)).await.unwrap();
    assert!(matches!(
        fx.app.client_state.read().await.songs.selected_option,
        Some(SongOption::Random)
    ));
}

#[tokio::test]
#[serial]
async fn option_navigation_reaches_all_curated_album_modes() {
    let mut fx = build_app().await;
    {
        let mut state = fx.app.client_state.write().await;
        state.songs.selected_option = Some(SongOption::RandomAlbum);
        state.songs.selected_index = Some(400);
        state.songs.scroll_offset = 390;
    }

    for expected in [
        SongOption::NewestAlbum,
        SongOption::RecentlyPlayed,
        SongOption::MostPlayed,
        SongOption::HighestRated,
    ] {
        fx.app.handle_key(key(KeyCode::Down)).await.unwrap();
        assert_eq!(
            fx.app.client_state.read().await.songs.selected_option,
            Some(expected)
        );
        let state = fx.app.client_state.read().await;
        assert_eq!(state.songs.selected_index, None);
        assert_eq!(state.songs.scroll_offset, 0);
    }

    fx.app.handle_key(key(KeyCode::Down)).await.unwrap();
    assert_eq!(
        fx.app.client_state.read().await.songs.selected_option,
        Some(SongOption::HighestRated),
        "the final option must stay selected at the lower boundary"
    );
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
    let cs = fx.app.client_state.read().await;
    assert!(
        cs.songs.selected_option.is_none(),
        "Up with no option must not invent a selection"
    );
    assert_eq!(cs.songs.focus, 0, "Up must not change focus pane");
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

// Proves songs_list() dispatches to random_album_songs (not starred/random)
// when RandomAlbum is selected, not just that RandomAlbum can be selected.
#[tokio::test]
#[serial]
async fn enter_with_random_album_selected_plays_from_random_album_songs() {
    let mut fx = build_app().await;
    {
        let mut ds = fx.app.daemon_state.write().await;
        ds.library.starred_songs = vec![song("starred0")];
        ds.library.random_songs = vec![song("rand0")];
        ds.library.random_album_songs = vec![song("alb0"), song("alb1")];
    }
    {
        let mut cs = fx.app.client_state.write().await;
        cs.songs.focus = 1;
        cs.songs.selected_option = Some(SongOption::RandomAlbum);
        cs.songs.selected_index = Some(1);
    }
    fx.app.handle_key(key(KeyCode::Enter)).await.unwrap();
    let ds = fx.app.daemon_state.read().await;
    assert!(ds.queue.iter().any(|s| s.id == "alb1"));
    assert!(!ds
        .queue
        .iter()
        .any(|s| s.id == "starred0" || s.id == "rand0"));
}

#[tokio::test]
#[serial]
async fn enter_with_curated_album_selected_uses_its_own_cached_songs() {
    let tempdir = common::tempdir();
    std::env::set_var("FERROSONIC_CONFIG_DIR", tempdir.path());
    let mut config = Config::new();
    config.daemon = false;
    let mut app = App::new(config);
    {
        let mut ds = app.daemon_state.write().await;
        ds.library
            .quick_play_album_songs
            .insert(QuickPlayAlbumKind::Newest, vec![song("new0"), song("new1")]);
    }
    {
        let mut cs = app.client_state.write().await;
        cs.page = ferrosonic::app::state::Page::QuickPlay;
        cs.songs.focus = 1;
        cs.songs.selected_option = Some(SongOption::NewestAlbum);
        cs.songs.selected_index = Some(1);
    }

    app.handle_key(key(KeyCode::Enter)).await.unwrap();

    let state = app.daemon_state.read().await;
    assert_eq!(state.queue.len(), 2);
    assert_eq!(state.queue[1].id, "new1");
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
    let cs = fx.app.client_state.read().await;
    let ds = fx.app.daemon_state.read().await;
    assert!(
        cs.songs.selected_index.is_none(),
        "'m' with no selection must not invent one"
    );
    assert!(ds.library.starred_songs.is_empty());
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
    let cs = fx.app.client_state.read().await;
    let ds = fx.app.daemon_state.read().await;
    assert_eq!(
        cs.songs.selected_index,
        Some(0),
        "'m' on valid selection must preserve selection"
    );
    assert_eq!(cs.songs.focus, 1, "'m' must not move focus");
    assert_eq!(
        ds.library.random_songs.first().map(|s| s.id.as_str()),
        Some("starme"),
        "'m' must not remove song from random list"
    );
}

#[tokio::test]
#[serial]
async fn unhandled_key_is_noop() {
    let mut fx = build_app().await;
    fx.app.handle_key(key(KeyCode::Insert)).await.unwrap();
    let cs = fx.app.client_state.read().await;
    assert_eq!(
        cs.songs.focus, 0,
        "unhandled key must not move focus from default"
    );
    assert!(
        cs.songs.selected_index.is_none(),
        "unhandled key must not invent a selection"
    );
    assert!(!cs.should_quit);
}

fn playlist(id: &str, name: &str) -> ferrosonic::subsonic::models::Playlist {
    ferrosonic::subsonic::models::Playlist {
        id: id.into(),
        name: name.into(),
        comment: None,
        owner: None,
        public: None,
        song_count: Some(1),
        duration: Some(60),
        cover_art: None,
    }
}

// Closes input_songs Up-init seam: `else if !songs_list().is_empty()` (delete `!`
// mutant) must still seed selection to 0 on a non-empty list with no selection.
#[tokio::test]
#[serial]
async fn up_in_song_pane_with_no_selection_inits_to_zero() {
    let mut fx = build_app().await;
    {
        let mut ds = fx.app.daemon_state.write().await;
        ds.library.random_songs = vec![song("a"), song("b")];
    }
    {
        let mut cs = fx.app.client_state.write().await;
        cs.songs.focus = 1;
        cs.songs.selected_option = Some(SongOption::Random);
    }
    fx.app.handle_key(key(KeyCode::Up)).await.unwrap();
    assert_eq!(
        fx.app.client_state.read().await.songs.selected_index,
        Some(0)
    );
}

// Closes input_songs Down-bound seam: `sel < max` (`< -> <=` mutant) must not
// step past the last index.
#[tokio::test]
#[serial]
async fn down_in_song_pane_at_last_index_stays() {
    let mut fx = build_app().await;
    {
        let mut ds = fx.app.daemon_state.write().await;
        ds.library.random_songs = vec![song("a"), song("b")];
    }
    {
        let mut cs = fx.app.client_state.write().await;
        cs.songs.focus = 1;
        cs.songs.selected_option = Some(SongOption::Random);
        cs.songs.selected_index = Some(1);
    }
    fx.app.handle_key(key(KeyCode::Down)).await.unwrap();
    assert_eq!(
        fx.app.client_state.read().await.songs.selected_index,
        Some(1)
    );
}

// Closes input_songs 'a'-arm seam (delete arm mutant): 'a' on a selected song
// with playlists present opens the playlist picker.
#[tokio::test]
#[serial]
async fn a_opens_playlist_picker_when_playlists_exist() {
    let mut fx = build_app().await;
    {
        let mut ds = fx.app.daemon_state.write().await;
        ds.library.random_songs = vec![song("a")];
        ds.library.playlists = vec![playlist("p0", "Mine")];
    }
    {
        let mut cs = fx.app.client_state.write().await;
        cs.songs.focus = 1;
        cs.songs.selected_option = Some(SongOption::Random);
        cs.songs.selected_index = Some(0);
    }
    fx.app.handle_key(key(KeyCode::Char('a'))).await.unwrap();
    assert!(
        fx.app.client_state.read().await.playlist_picker.active,
        "'a' must open the playlist picker"
    );
}

// Closes input_songs Enter-filter seam: `idx < len` (`< -> <=` mutant) must reject
// a stale out-of-range selection instead of enqueuing+playing it.
#[tokio::test]
#[serial]
async fn enter_with_out_of_range_selection_does_not_enqueue() {
    let td = common::TestDaemon::new().await;
    let cfg = td.state.read().await.config.clone();
    let mut app = App::with_remote_client(
        std::sync::Arc::new(ferrosonic::ipc::InProcessClient::new(td.core.clone())),
        cfg,
    );
    {
        let mut ds = app.daemon_state.write().await;
        ds.library.random_songs = vec![song("a"), song("b")];
    }
    {
        let mut cs = app.client_state.write().await;
        cs.page = ferrosonic::app::state::Page::QuickPlay;
        cs.songs.focus = 1;
        cs.songs.selected_option = Some(SongOption::Random);
        cs.songs.selected_index = Some(2);
    }
    app.handle_key(key(KeyCode::Enter)).await.unwrap();
    assert!(
        td.state.read().await.queue.is_empty(),
        "Enter on an out-of-range selection must not enqueue"
    );
}

#[tokio::test]
async fn returning_to_quick_play_retains_random_album_without_refresh() {
    let rec = common::RecordingClient::new();
    let mut app = App::with_remote_client(rec.clone(), Config::new());
    app.client_state.write().await.songs.selected_option = Some(SongOption::RandomAlbum);
    app.handle_key(key(KeyCode::F(3))).await.unwrap();
    app.handle_key(key(KeyCode::F(1))).await.unwrap();
    app.handle_key(key(KeyCode::F(3))).await.unwrap();
    assert_eq!(
        app.client_state.read().await.songs.selected_option,
        Some(SongOption::RandomAlbum)
    );
    assert!(rec.requests().await.is_empty());
}

#[tokio::test]
#[serial]
async fn highlighted_song_requires_song_pane_focus_on_quick_play() {
    use ferrosonic::app::state::AppState;

    let mut fx = build_app().await;
    {
        let mut cs = fx.app.client_state.write().await;
        cs.songs.selected_option = Some(SongOption::Starred);
        cs.songs.selected_index = Some(0);
        cs.songs.focus = 0;
    }
    {
        let mut ds = fx.app.daemon_state.write().await;
        ds.library.starred_songs = vec![song("s1")];
    }

    {
        let ds = fx.app.daemon_state.read().await;
        let mut cs = fx.app.client_state.write().await;
        let state = AppState {
            daemon: &ds,
            client: &mut cs,
        };
        assert!(
            state.highlighted_song().is_none(),
            "with the option pane focused, no song row is highlighted"
        );
    }

    fx.app.client_state.write().await.songs.focus = 1;
    {
        let ds = fx.app.daemon_state.read().await;
        let mut cs = fx.app.client_state.write().await;
        let state = AppState {
            daemon: &ds,
            client: &mut cs,
        };
        assert_eq!(
            state.highlighted_song().map(|(id, _)| id),
            Some("s1".to_string()),
            "with the song pane focused, the highlighted row resolves"
        );
    }
}
