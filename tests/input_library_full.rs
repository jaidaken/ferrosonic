//! Exhaustive library-page input handlers: every branch in input_library.rs.

mod common;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ferrosonic::app::state::FilterScope;
use ferrosonic::app::App;
use ferrosonic::config::Config;
use ferrosonic::subsonic::models::{Album, Artist, Child, SearchResult3};
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

fn artist(id: &str, name: &str) -> Artist {
    Artist {
        id: id.into(),
        name: name.into(),
        album_count: Some(1),
        cover_art: None,
    }
}

fn album(id: &str, name: &str) -> Album {
    Album {
        id: id.into(),
        name: name.into(),
        artist: Some("X".into()),
        artist_id: Some("a0".into()),
        cover_art: None,
        song_count: Some(5),
        original_release_date: None,
        duration: Some(1200),
        year: Some(2020),
        genre: None,
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
    app.handle_key(key(KeyCode::F(1))).await.unwrap();
    AppFixture {
        app,
        _tempdir: tempdir,
    }
}

#[tokio::test]
#[serial]
async fn filter_backspace_to_empty_clears_search_results() {
    let mut fx = build_app().await;
    fx.app.handle_key(key(KeyCode::Char('/'))).await.unwrap();
    fx.app.handle_key(key(KeyCode::Char('x'))).await.unwrap();
    {
        let mut cs = fx.app.client_state.write().await;
        cs.artists.search_results = Some(SearchResult3::default());
    }
    fx.app.handle_key(key(KeyCode::Backspace)).await.unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(1), async {
        loop {
            if fx
                .app
                .client_state
                .read()
                .await
                .artists
                .search_results
                .is_none()
            {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("clear-search task did not clear search_results");
    let cs = fx.app.client_state.read().await;
    assert!(
        cs.artists.search_results.is_none(),
        "backspace to empty filter should clear search results"
    );
}

#[tokio::test]
#[serial]
async fn typing_filter_chars_bumps_search_gen() {
    let mut fx = build_app().await;
    fx.app.handle_key(key(KeyCode::Char('/'))).await.unwrap();
    let before = fx.app.client_state.read().await.artists.search_gen;
    fx.app.handle_key(key(KeyCode::Char('q'))).await.unwrap();
    fx.app.handle_key(key(KeyCode::Char('u'))).await.unwrap();
    fx.app.handle_key(key(KeyCode::Char('e'))).await.unwrap();
    let after = fx.app.client_state.read().await.artists.search_gen;
    assert!(
        after >= before + 3,
        "three keystrokes should advance search_gen by >= 3"
    );
}

#[tokio::test]
#[serial]
async fn filter_active_unknown_key_is_ignored() {
    let mut fx = build_app().await;
    fx.app.handle_key(key(KeyCode::Char('/'))).await.unwrap();
    fx.app.handle_key(key(KeyCode::Insert)).await.unwrap();
    let cs = fx.app.client_state.read().await;
    assert!(cs.artists.filter_active);
    assert!(cs.artists.filter.is_empty());
}

#[tokio::test]
#[serial]
async fn esc_outside_filter_resets_full_library_state() {
    let mut fx = build_app().await;
    {
        let mut cs = fx.app.client_state.write().await;
        cs.artists.filter = "abc".into();
        cs.artists.expanded.insert("a0".into());
        cs.artists.filter_scope = FilterScope::Albums;
        cs.artists.search_results = Some(SearchResult3::default());
    }
    fx.app.handle_key(key(KeyCode::Esc)).await.unwrap();
    let cs = fx.app.client_state.read().await;
    assert!(cs.artists.filter.is_empty());
    assert!(cs.artists.expanded.is_empty());
    assert!(cs.artists.search_results.is_none());
    assert_eq!(cs.artists.filter_scope, FilterScope::Artists);
    assert_eq!(cs.artists.selected_index, Some(0));
}

#[tokio::test]
#[serial]
async fn tab_cycles_focus_between_zero_and_one() {
    let mut fx = build_app().await;
    fx.app.handle_key(key(KeyCode::Tab)).await.unwrap();
    assert_eq!(fx.app.client_state.read().await.artists.focus, 1);
    fx.app.handle_key(key(KeyCode::Tab)).await.unwrap();
    assert_eq!(fx.app.client_state.read().await.artists.focus, 0);
}

#[tokio::test]
#[serial]
async fn left_forces_focus_to_tree() {
    let mut fx = build_app().await;
    {
        let mut cs = fx.app.client_state.write().await;
        cs.artists.focus = 1;
    }
    fx.app.handle_key(key(KeyCode::Left)).await.unwrap();
    assert_eq!(fx.app.client_state.read().await.artists.focus, 0);
}

#[tokio::test]
#[serial]
async fn right_with_no_songs_is_noop() {
    let mut fx = build_app().await;
    fx.app.handle_key(key(KeyCode::Right)).await.unwrap();
    assert_eq!(fx.app.client_state.read().await.artists.focus, 0);
}

#[tokio::test]
#[serial]
async fn right_with_songs_no_selected_song_initializes_to_zero() {
    let mut fx = build_app().await;
    {
        let mut cs = fx.app.client_state.write().await;
        cs.artists.songs = vec![song("s")];
    }
    fx.app.handle_key(key(KeyCode::Right)).await.unwrap();
    let cs = fx.app.client_state.read().await;
    assert_eq!(cs.artists.focus, 1);
    assert_eq!(cs.artists.selected_song, Some(0));
}

#[tokio::test]
#[serial]
async fn up_with_no_index_and_items_initializes_selection() {
    let mut fx = build_app().await;
    {
        let mut ds = fx.app.daemon_state.write().await;
        ds.library.artists = vec![artist("a0", "A")];
    }
    fx.app.handle_key(key(KeyCode::Up)).await.unwrap();
    let cs = fx.app.client_state.read().await;
    assert_eq!(cs.artists.selected_index, Some(0));
}

#[tokio::test]
#[serial]
async fn up_in_song_pane_no_selection_initializes_to_zero() {
    let mut fx = build_app().await;
    {
        let mut cs = fx.app.client_state.write().await;
        cs.artists.focus = 1;
        cs.artists.songs = vec![song("s0"), song("s1")];
    }
    fx.app.handle_key(key(KeyCode::Up)).await.unwrap();
    assert_eq!(
        fx.app.client_state.read().await.artists.selected_song,
        Some(0)
    );
}

#[tokio::test]
#[serial]
async fn up_in_song_pane_at_top_stays() {
    let mut fx = build_app().await;
    {
        let mut cs = fx.app.client_state.write().await;
        cs.artists.focus = 1;
        cs.artists.songs = vec![song("s0"), song("s1")];
        cs.artists.selected_song = Some(0);
    }
    fx.app.handle_key(key(KeyCode::Up)).await.unwrap();
    assert_eq!(
        fx.app.client_state.read().await.artists.selected_song,
        Some(0)
    );
}

#[tokio::test]
#[serial]
async fn down_in_song_pane_no_selection_initializes_to_zero() {
    let mut fx = build_app().await;
    {
        let mut cs = fx.app.client_state.write().await;
        cs.artists.focus = 1;
        cs.artists.songs = vec![song("s0"), song("s1")];
    }
    fx.app.handle_key(key(KeyCode::Down)).await.unwrap();
    assert_eq!(
        fx.app.client_state.read().await.artists.selected_song,
        Some(0)
    );
}

#[tokio::test]
#[serial]
async fn down_in_song_pane_past_end_stays_at_max() {
    let mut fx = build_app().await;
    {
        let mut cs = fx.app.client_state.write().await;
        cs.artists.focus = 1;
        cs.artists.songs = vec![song("s0"), song("s1")];
        cs.artists.selected_song = Some(1);
    }
    fx.app.handle_key(key(KeyCode::Down)).await.unwrap();
    assert_eq!(
        fx.app.client_state.read().await.artists.selected_song,
        Some(1)
    );
}

#[tokio::test]
#[serial]
async fn down_with_empty_tree_does_not_panic() {
    let mut fx = build_app().await;
    fx.app.handle_key(key(KeyCode::Down)).await.unwrap();
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
async fn j_advances_like_down_in_tree() {
    let mut fx = build_app().await;
    {
        let mut ds = fx.app.daemon_state.write().await;
        ds.library.artists = vec![artist("a0", "A"), artist("a1", "B")];
    }
    fx.app.handle_key(key(KeyCode::Char('j'))).await.unwrap();
    let cs = fx.app.client_state.read().await;
    assert_eq!(cs.artists.selected_index, Some(0));
}

#[tokio::test]
#[serial]
async fn k_reverses_like_up_in_tree() {
    let mut fx = build_app().await;
    {
        let mut ds = fx.app.daemon_state.write().await;
        ds.library.artists = vec![artist("a0", "A"), artist("a1", "B")];
    }
    fx.app.handle_key(key(KeyCode::Down)).await.unwrap();
    fx.app.handle_key(key(KeyCode::Down)).await.unwrap();
    fx.app.handle_key(key(KeyCode::Char('k'))).await.unwrap();
    let cs = fx.app.client_state.read().await;
    assert_eq!(cs.artists.selected_index, Some(0));
}

#[tokio::test]
#[serial]
async fn t_on_song_in_search_mode_replays_the_song() {
    let mut fx = build_app().await;
    {
        let mut cs = fx.app.client_state.write().await;
        cs.artists.filter = "x".into();
        cs.artists.filter_scope = FilterScope::Songs;
        cs.artists.search_results = Some(SearchResult3 {
            artist: vec![],
            album: vec![],
            song: vec![song("s0")],
        });
        cs.artists.selected_index = Some(0);
    }
    fx.app.handle_key(key(KeyCode::Char('t'))).await.unwrap();
    let ds = fx.app.daemon_state.read().await;
    assert!(ds.queue.iter().any(|s| s.id == "s0"));
}

#[tokio::test]
#[serial]
async fn t_on_album_in_cache_attempts_shuffle_play() {
    let mut fx = build_app().await;
    {
        let mut cs = fx.app.client_state.write().await;
        cs.artists.filter = "x".into();
        cs.artists.filter_scope = FilterScope::Albums;
        cs.artists.search_results = Some(SearchResult3 {
            artist: vec![],
            album: vec![album("alb0", "A")],
            song: vec![],
        });
        cs.artists.selected_index = Some(0);
    }
    fx.app.handle_key(key(KeyCode::Char('t'))).await.unwrap();
}

#[tokio::test]
#[serial]
async fn enter_on_artist_not_in_cache_attempts_load() {
    let mut fx = build_app().await;
    {
        let mut ds = fx.app.daemon_state.write().await;
        ds.library.artists = vec![artist("a0", "A")];
    }
    {
        let mut cs = fx.app.client_state.write().await;
        cs.artists.selected_index = Some(0);
    }
    fx.app.handle_key(key(KeyCode::Enter)).await.unwrap();
}

#[tokio::test]
#[serial]
async fn enter_on_collapsed_artist_in_cache_expands() {
    let mut fx = build_app().await;
    {
        let mut ds = fx.app.daemon_state.write().await;
        ds.library.artists = vec![artist("a0", "A")];
        ds.library
            .albums_cache
            .insert("a0".into(), vec![album("alb0", "Album A")]);
    }
    {
        let mut cs = fx.app.client_state.write().await;
        cs.artists.selected_index = Some(0);
    }
    fx.app.handle_key(key(KeyCode::Enter)).await.unwrap();
    let cs = fx.app.client_state.read().await;
    assert!(cs.artists.expanded.contains("a0"));
}

#[tokio::test]
#[serial]
async fn enter_on_album_with_no_songs_notifies_error() {
    let mut fx = build_app().await;
    {
        let mut cs = fx.app.client_state.write().await;
        cs.artists.filter = "x".into();
        cs.artists.filter_scope = FilterScope::Albums;
        cs.artists.search_results = Some(SearchResult3 {
            artist: vec![],
            album: vec![album("alb0", "A")],
            song: vec![],
        });
        cs.artists.selected_index = Some(0);
    }
    fx.app.handle_key(key(KeyCode::Enter)).await.unwrap();
}

#[tokio::test]
#[serial]
async fn enter_on_song_in_search_mode_plays_it() {
    let mut fx = build_app().await;
    {
        let mut cs = fx.app.client_state.write().await;
        cs.artists.filter = "x".into();
        cs.artists.filter_scope = FilterScope::Songs;
        cs.artists.search_results = Some(SearchResult3 {
            artist: vec![],
            album: vec![],
            song: vec![song("s0")],
        });
        cs.artists.selected_index = Some(0);
    }
    fx.app.handle_key(key(KeyCode::Enter)).await.unwrap();
}

#[tokio::test]
#[serial]
async fn enter_in_song_pane_with_valid_index_plays() {
    let mut fx = build_app().await;
    {
        let mut cs = fx.app.client_state.write().await;
        cs.artists.focus = 1;
        cs.artists.songs = vec![song("s0"), song("s1")];
        cs.artists.selected_song = Some(1);
    }
    fx.app.handle_key(key(KeyCode::Enter)).await.unwrap();
    let ds = fx.app.daemon_state.read().await;
    assert!(ds.queue.iter().any(|s| s.id == "s1"));
}

#[tokio::test]
#[serial]
async fn enter_in_song_pane_with_oob_index_no_op() {
    let mut fx = build_app().await;
    {
        let mut cs = fx.app.client_state.write().await;
        cs.artists.focus = 1;
        cs.artists.songs = vec![song("s0")];
        cs.artists.selected_song = Some(99);
    }
    fx.app.handle_key(key(KeyCode::Enter)).await.unwrap();
    let ds = fx.app.daemon_state.read().await;
    assert!(ds.queue.is_empty());
}

#[tokio::test]
#[serial]
async fn e_with_no_songs_and_no_filter_is_noop() {
    let mut fx = build_app().await;
    fx.app.handle_key(key(KeyCode::Char('e'))).await.unwrap();
    let ds = fx.app.daemon_state.read().await;
    assert!(ds.queue.is_empty());
}

#[tokio::test]
#[serial]
async fn e_in_search_mode_for_song_collects_and_appends() {
    let mut fx = build_app().await;
    {
        let mut cs = fx.app.client_state.write().await;
        cs.artists.filter = "x".into();
        cs.artists.filter_scope = FilterScope::Songs;
        cs.artists.search_results = Some(SearchResult3 {
            artist: vec![],
            album: vec![],
            song: vec![song("s0"), song("s1")],
        });
        cs.artists.selected_index = Some(0);
    }
    fx.app.handle_key(key(KeyCode::Char('e'))).await.unwrap();
    let ds = fx.app.daemon_state.read().await;
    assert!(ds.queue.iter().any(|s| s.id == "s0"));
}

#[tokio::test]
#[serial]
async fn i_with_no_songs_and_no_filter_is_noop() {
    let mut fx = build_app().await;
    fx.app.handle_key(key(KeyCode::Char('i'))).await.unwrap();
    let ds = fx.app.daemon_state.read().await;
    assert!(ds.queue.is_empty());
}

#[tokio::test]
#[serial]
async fn i_in_song_pane_without_position_uses_append_mode() {
    let mut fx = build_app().await;
    {
        let mut cs = fx.app.client_state.write().await;
        cs.artists.focus = 1;
        cs.artists.songs = vec![song("only")];
        cs.artists.selected_song = Some(0);
    }
    fx.app.handle_key(key(KeyCode::Char('i'))).await.unwrap();
    let ds = fx.app.daemon_state.read().await;
    assert!(ds.queue.iter().any(|s| s.id == "only"));
}

#[tokio::test]
#[serial]
async fn i_in_search_mode_for_song_inserts_next() {
    let mut fx = build_app().await;
    {
        let mut ds = fx.app.daemon_state.write().await;
        ds.queue.push(song("existing"));
        ds.queue_position = Some(0);
    }
    {
        let mut cs = fx.app.client_state.write().await;
        cs.artists.filter = "x".into();
        cs.artists.filter_scope = FilterScope::Songs;
        cs.artists.search_results = Some(SearchResult3 {
            artist: vec![],
            album: vec![],
            song: vec![song("new")],
        });
        cs.artists.selected_index = Some(0);
    }
    fx.app.handle_key(key(KeyCode::Char('i'))).await.unwrap();
    let ds = fx.app.daemon_state.read().await;
    assert!(ds.queue.iter().any(|s| s.id == "new"));
}

#[tokio::test]
#[serial]
async fn i_with_artist_song_list_inserts_after_current_position() {
    let mut fx = build_app().await;
    {
        let mut ds = fx.app.daemon_state.write().await;
        ds.queue.push(song("first"));
        ds.queue_position = Some(0);
    }
    {
        let mut cs = fx.app.client_state.write().await;
        cs.artists.songs = vec![song("inserted-a"), song("inserted-b")];
        cs.artists.focus = 0;
    }
    fx.app.handle_key(key(KeyCode::Char('i'))).await.unwrap();
    let ds = fx.app.daemon_state.read().await;
    assert!(ds.queue.iter().any(|s| s.id == "inserted-a"));
}

#[tokio::test]
#[serial]
async fn m_on_song_in_search_mode_dispatches_star_toggle() {
    let mut fx = build_app().await;
    {
        let mut cs = fx.app.client_state.write().await;
        cs.artists.filter = "x".into();
        cs.artists.filter_scope = FilterScope::Songs;
        cs.artists.search_results = Some(SearchResult3 {
            artist: vec![],
            album: vec![],
            song: vec![song("starme")],
        });
        cs.artists.selected_index = Some(0);
    }
    fx.app.handle_key(key(KeyCode::Char('m'))).await.unwrap();
}

#[tokio::test]
#[serial]
async fn m_on_non_song_search_item_is_noop() {
    let mut fx = build_app().await;
    {
        let mut cs = fx.app.client_state.write().await;
        cs.artists.filter = "x".into();
        cs.artists.filter_scope = FilterScope::Albums;
        cs.artists.search_results = Some(SearchResult3 {
            artist: vec![],
            album: vec![album("alb0", "A")],
            song: vec![],
        });
        cs.artists.selected_index = Some(0);
    }
    fx.app.handle_key(key(KeyCode::Char('m'))).await.unwrap();
}

#[tokio::test]
#[serial]
async fn unhandled_key_is_silently_ignored() {
    let mut fx = build_app().await;
    fx.app.handle_key(key(KeyCode::F(5))).await.unwrap();
    let cs = fx.app.client_state.read().await;
    assert!(!cs.artists.filter_active);
}

#[tokio::test]
#[serial]
async fn slash_after_typing_in_search_mode_appends_literal_then_continues() {
    let mut fx = build_app().await;
    fx.app.handle_key(key(KeyCode::Char('/'))).await.unwrap();
    fx.app.handle_key(key(KeyCode::Char('a'))).await.unwrap();
    let gen_before = fx.app.client_state.read().await.artists.search_gen;
    fx.app.handle_key(key(KeyCode::Char('/'))).await.unwrap();
    let cs = fx.app.client_state.read().await;
    assert_eq!(cs.artists.filter, "a/");
    assert!(cs.artists.search_gen > gen_before);
}
