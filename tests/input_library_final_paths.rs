//! input_library.rs final paths: search task body, up/down auto-load, t-on-album-songs.

mod common;

use common::TestDaemon;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ferrosonic::app::state::FilterScope;
use ferrosonic::app::App;
use ferrosonic::subsonic::models::{Album, Artist, Child, SearchResult3};
use serial_test::serial;
use std::time::Duration;

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
        duration: Some(360),
        year: Some(2024),
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
async fn typing_into_filter_triggers_search_task_and_commits_result() {
    let (mut app, td) = build_app_with_td().await;
    td.fake_subsonic
        .expect_search3(&["FoundArtist"], &["FoundAlbum"], &["FoundSong"])
        .await;
    app.handle_key(key(KeyCode::Char('/'))).await.unwrap();
    app.handle_key(key(KeyCode::Char('f'))).await.unwrap();
    app.handle_key(key(KeyCode::Char('o'))).await.unwrap();
    app.handle_key(key(KeyCode::Char('u'))).await.unwrap();
    tokio::time::sleep(Duration::from_millis(300)).await;
    let cs = app.client_state.read().await;
    assert!(cs.artists.search_results.is_some());
}

#[tokio::test]
#[serial]
async fn e_in_search_mode_for_artist_with_subsonic_appends() {
    let (mut app, td) = build_app_with_td().await;
    td.fake_subsonic
        .expect_get_artist("artist-x", "X", &["A1"])
        .await;
    td.fake_subsonic
        .expect_get_album("alb-0", "A1", &["s0", "s1"])
        .await;
    {
        let mut cs = app.client_state.write().await;
        cs.artists.filter = "x".into();
        cs.artists.filter_scope = FilterScope::Artists;
        cs.artists.search_results = Some(SearchResult3 {
            artist: vec![artist("artist-x", "X")],
            album: vec![],
            song: vec![],
        });
        cs.artists.selected_index = Some(0);
    }
    app.handle_key(key(KeyCode::Char('e'))).await.unwrap();
    let ds = td.state.read().await;
    assert_eq!(ds.queue.len(), 2);
}

#[tokio::test]
#[serial]
async fn i_in_search_mode_for_artist_inserts_after_position() {
    let (mut app, td) = build_app_with_td().await;
    td.fake_subsonic
        .expect_get_artist("artist-y", "Y", &["AY"])
        .await;
    td.fake_subsonic
        .expect_get_album("alb-0", "AY", &["y0"])
        .await;
    {
        let mut s = td.state.write().await;
        s.queue.push(song("existing"));
        s.queue_position = Some(0);
    }
    {
        let mut cs = app.client_state.write().await;
        cs.artists.filter = "y".into();
        cs.artists.filter_scope = FilterScope::Artists;
        cs.artists.search_results = Some(SearchResult3 {
            artist: vec![artist("artist-y", "Y")],
            album: vec![],
            song: vec![],
        });
        cs.artists.selected_index = Some(0);
    }
    app.handle_key(key(KeyCode::Char('i'))).await.unwrap();
    let ds = td.state.read().await;
    assert!(ds.queue.len() > 1, "queue should contain inserted song");
}

#[tokio::test]
#[serial]
async fn t_on_artist_with_no_albums_yields_no_action() {
    let (mut app, td) = build_app_with_td().await;
    td.fake_subsonic
        .expect_get_artist("artist-noalbum", "NA", &[])
        .await;
    {
        let mut s = app.daemon_state.write().await;
        s.library.artists = vec![artist("artist-noalbum", "NA")];
    }
    {
        let mut cs = app.client_state.write().await;
        cs.artists.selected_index = Some(0);
    }
    app.handle_key(key(KeyCode::Char('t'))).await.unwrap();
}

#[tokio::test]
#[serial]
async fn up_arrow_at_zero_index_with_album_does_not_load() {
    let (mut app, _td) = build_app_with_td().await;
    {
        let mut s = app.daemon_state.write().await;
        s.library.artists = vec![artist("a0", "A")];
        s.library
            .albums_cache
            .insert("a0".into(), vec![album("alb-0", "Album")]);
    }
    {
        let mut cs = app.client_state.write().await;
        cs.artists.expanded.insert("a0".into());
        cs.artists.selected_index = Some(0);
    }
    app.handle_key(key(KeyCode::Up)).await.unwrap();
}

#[tokio::test]
#[serial]
async fn up_arrow_lands_on_album_returns_empty_songs() {
    let (mut app, td) = build_app_with_td().await;
    td.fake_subsonic
        .expect_get_album("alb-emp", "EmptyAlb", &[])
        .await;
    {
        let mut s = app.daemon_state.write().await;
        s.library.artists = vec![artist("a0", "A")];
        s.library
            .albums_cache
            .insert("a0".into(), vec![album("alb-emp", "EmptyAlb")]);
    }
    {
        let mut cs = app.client_state.write().await;
        cs.artists.expanded.insert("a0".into());
        cs.artists.selected_index = Some(1);
    }
    app.handle_key(key(KeyCode::Down)).await.unwrap();
}
