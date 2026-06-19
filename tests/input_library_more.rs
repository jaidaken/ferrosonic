//! More input_library.rs branches: expanded artist + album tree items.

mod common;

use common::TestDaemon;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ferrosonic::app::App;
use ferrosonic::subsonic::models::{Album, Artist, Child, SearchResult3};
use serial_test::serial;

fn key(code: KeyCode) -> KeyEvent {
    let mut k = KeyEvent::new(code, KeyModifiers::NONE);
    k.kind = KeyEventKind::Press;
    k
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
        song_count: Some(3),
        original_release_date: None,
        duration: Some(540),
        year: Some(2020),
        genre: None,
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

async fn build_app_with_td() -> (App, TestDaemon) {
    let td = TestDaemon::new().await;
    let cfg = td.state.read().await.config.clone();
    let app = App::with_remote_client(
        std::sync::Arc::new(ferrosonic::ipc::InProcessClient::new(td.core.clone())),
        cfg,
    );
    {
        let mut cs = app.client_state.write().await;
        cs.page = ferrosonic::app::state::Page::Library;
    }
    (app, td)
}

#[tokio::test]
#[serial]
async fn enter_on_album_in_tree_loads_and_plays() {
    let (mut app, td) = build_app_with_td().await;
    td.fake_subsonic
        .expect_get_album("alb-x", "Album X", &["s0", "s1"])
        .await;
    {
        let mut s = app.daemon_state.write().await;
        s.library.artists = vec![artist("a0", "Artist")];
        s.library
            .albums_cache
            .insert("a0".into(), vec![album("alb-x", "Album X")]);
    }
    {
        let mut cs = app.client_state.write().await;
        cs.artists.expanded.insert("a0".into());
        cs.artists.selected_index = Some(1);
    }
    app.handle_key(key(KeyCode::Enter)).await.unwrap();
    let cs = app.client_state.read().await;
    assert_eq!(cs.artists.focus, 1);
    assert!(!cs.artists.songs.is_empty());
}

#[tokio::test]
#[serial]
async fn t_on_album_in_tree_shuffles_and_plays() {
    let (mut app, td) = build_app_with_td().await;
    td.fake_subsonic
        .expect_get_album("alb-y", "Album Y", &["yy0", "yy1", "yy2"])
        .await;
    {
        let mut s = app.daemon_state.write().await;
        s.library.artists = vec![artist("a0", "Artist")];
        s.library
            .albums_cache
            .insert("a0".into(), vec![album("alb-y", "Album Y")]);
    }
    {
        let mut cs = app.client_state.write().await;
        cs.artists.expanded.insert("a0".into());
        cs.artists.selected_index = Some(1);
    }
    app.handle_key(key(KeyCode::Char('t'))).await.unwrap();
}

#[tokio::test]
#[serial]
async fn up_arrow_on_album_in_tree_auto_loads_songs() {
    let (mut app, td) = build_app_with_td().await;
    td.fake_subsonic
        .expect_get_album("alb-z", "Album Z", &["z0"])
        .await;
    {
        let mut s = app.daemon_state.write().await;
        s.library.artists = vec![artist("a0", "Artist")];
        s.library.albums_cache.insert(
            "a0".into(),
            vec![album("alb-1", "A1"), album("alb-z", "Album Z")],
        );
    }
    {
        let mut cs = app.client_state.write().await;
        cs.artists.expanded.insert("a0".into());
        cs.artists.selected_index = Some(3);
    }
    app.handle_key(key(KeyCode::Up)).await.unwrap();
    let cs = app.client_state.read().await;
    assert!(!cs.artists.songs.is_empty());
}

#[tokio::test]
#[serial]
async fn highlighting_search_song_loads_its_album_with_that_song_preselected() {
    let (mut app, td) = build_app_with_td().await;
    td.fake_subsonic
        .expect_get_album("alb-x", "Album X", &["First", "Second", "Third"])
        .await;
    {
        let mut cs = app.client_state.write().await;
        cs.artists.focus = 0;
        cs.artists.filter = "second".into();
        let mut s = song("song-1");
        s.title = "Second".into();
        s.parent = Some("alb-x".into());
        cs.artists.search_results = Some(SearchResult3 {
            artist: vec![],
            album: vec![],
            song: vec![s],
        });
        cs.artists.selected_index = None;
    }
    // Down lands on the lone song row; its album fills the pane.
    app.handle_key(key(KeyCode::Down)).await.unwrap();
    {
        let cs = app.client_state.read().await;
        assert_eq!(
            cs.artists.songs.len(),
            3,
            "the song's whole album loads into the pane"
        );
        assert_eq!(
            cs.artists.selected_song,
            Some(1),
            "the matched song is pre-selected within the album, not the first track"
        );
    }
    // Right moves focus to the pane and keeps the matched song selected.
    app.handle_key(key(KeyCode::Right)).await.unwrap();
    let cs = app.client_state.read().await;
    assert_eq!(cs.artists.focus, 1, "Right focuses the song pane");
    assert_eq!(
        cs.artists.selected_song,
        Some(1),
        "Right lands on the matched song, not the first"
    );
}

#[tokio::test]
#[serial]
async fn t_on_song_in_tree_plays_single() {
    let (mut app, td) = build_app_with_td().await;
    {
        let mut cs = app.client_state.write().await;
        cs.artists.filter = "song".into();
        cs.artists.search_results = Some(SearchResult3 {
            artist: vec![],
            album: vec![],
            song: vec![song("only-song")],
        });
        cs.artists.selected_index = Some(0);
    }
    app.handle_key(key(KeyCode::Char('t'))).await.unwrap();
    let ds = td.state.read().await;
    assert!(ds.queue.iter().any(|s| s.id == "only-song"));
}

#[tokio::test]
#[serial]
async fn e_with_loaded_artist_songs_appends_all() {
    let (mut app, _td) = build_app_with_td().await;
    {
        let mut cs = app.client_state.write().await;
        cs.artists.focus = 0;
        cs.artists.songs = vec![song("a"), song("b"), song("c")];
    }
    app.handle_key(key(KeyCode::Char('e'))).await.unwrap();
    let ds = _td.state.read().await;
    assert_eq!(ds.queue.len(), 3);
}

#[tokio::test]
#[serial]
async fn i_with_loaded_artist_songs_inserts_after_current() {
    let (mut app, td) = build_app_with_td().await;
    {
        let mut s = td.state.write().await;
        s.queue.push(song("first"));
        s.queue_position = Some(0);
    }
    {
        let mut cs = app.client_state.write().await;
        cs.artists.focus = 0;
        cs.artists.songs = vec![song("mid-a"), song("mid-b")];
    }
    app.handle_key(key(KeyCode::Char('i'))).await.unwrap();
    let ds = td.state.read().await;
    assert!(ds.queue.iter().any(|s| s.id == "mid-a"));
}
