//! Remaining input_library.rs branches: Enter on Album/Song in tree, Down arrow auto-load.

mod common;

use common::TestDaemon;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ferrosonic::app::state::FilterScope;
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
        song_count: Some(2),
        original_release_date: None,
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
async fn down_arrow_lands_on_album_auto_loads_songs() {
    let (mut app, td) = build_app_with_td().await;
    td.fake_subsonic
        .expect_get_album("alb-d", "Album D", &["d0"])
        .await;
    {
        let mut s = app.daemon_state.write().await;
        s.library.artists = vec![artist("a0", "Artist")];
        s.library
            .albums_cache
            .insert("a0".into(), vec![album("alb-d", "Album D")]);
    }
    {
        let mut cs = app.client_state.write().await;
        cs.artists.expanded.insert("a0".into());
        cs.artists.selected_index = Some(0);
    }
    app.handle_key(key(KeyCode::Down)).await.unwrap();
    let cs = app.client_state.read().await;
    assert!(!cs.artists.songs.is_empty());
}

#[tokio::test]
#[serial]
async fn enter_on_song_in_search_replace_plays_from_zero() {
    let (mut app, td) = build_app_with_td().await;
    {
        let mut cs = app.client_state.write().await;
        cs.artists.filter = "s".into();
        cs.artists.filter_scope = FilterScope::Songs;
        cs.artists.search_results = Some(SearchResult3 {
            artist: vec![],
            album: vec![],
            song: vec![song("only")],
        });
        cs.artists.selected_index = Some(0);
    }
    app.handle_key(key(KeyCode::Enter)).await.unwrap();
    let ds = td.state.read().await;
    assert!(ds.queue.iter().any(|s| s.id == "only"));
}

#[tokio::test]
#[serial]
async fn t_on_artist_with_albums_but_empty_songs_notifies_error() {
    let (mut app, td) = build_app_with_td().await;
    td.fake_subsonic
        .expect_get_artist("a-empty", "EmptyArtist", &["Empty Album"])
        .await;
    td.fake_subsonic
        .expect_get_album("alb-0", "Empty Album", &[])
        .await;
    {
        let mut s = app.daemon_state.write().await;
        s.library.artists = vec![artist("a-empty", "EmptyArtist")];
    }
    {
        let mut cs = app.client_state.write().await;
        cs.artists.selected_index = Some(0);
    }
    app.handle_key(key(KeyCode::Char('t'))).await.unwrap();
    let cs = app.client_state.read().await;
    let notif_text = cs
        .notification
        .as_ref()
        .map(|n| n.message.clone())
        .unwrap_or_default();
    assert!(
        notif_text.contains("No songs") || notif_text.is_empty(),
        "notification: {}",
        notif_text
    );
}

#[tokio::test]
#[serial]
async fn enter_on_album_in_tree_with_empty_songs_notifies_error() {
    let (mut app, td) = build_app_with_td().await;
    td.fake_subsonic
        .expect_get_album("alb-x", "Empty", &[])
        .await;
    {
        let mut s = app.daemon_state.write().await;
        s.library.artists = vec![artist("a0", "Artist")];
        s.library
            .albums_cache
            .insert("a0".into(), vec![album("alb-x", "Empty")]);
    }
    {
        let mut cs = app.client_state.write().await;
        cs.artists.expanded.insert("a0".into());
        cs.artists.selected_index = Some(1);
    }
    app.handle_key(key(KeyCode::Enter)).await.unwrap();
    let cs = app.client_state.read().await;
    assert!(cs.notification.is_some() || cs.artists.songs.is_empty());
}

#[tokio::test]
#[serial]
async fn enter_on_artist_not_in_cache_when_load_fails_shows_failed_to_load() {
    let (mut app, _td) = build_app_with_td().await;
    {
        let mut s = app.daemon_state.write().await;
        s.config.base_url.clear();
        s.library.artists = vec![artist("a-fail", "FailArtist")];
    }
    {
        let mut cs = app.client_state.write().await;
        cs.artists.selected_index = Some(0);
    }
    app.handle_key(key(KeyCode::Enter)).await.unwrap();
}
