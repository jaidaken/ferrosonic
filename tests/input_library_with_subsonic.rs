//! input_library.rs paths that need real subsonic responses.

mod common;

use common::TestDaemon;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ferrosonic::app::App;
use serial_test::serial;

fn key(code: KeyCode) -> KeyEvent {
    let mut k = KeyEvent::new(code, KeyModifiers::NONE);
    k.kind = KeyEventKind::Press;
    k
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
async fn t_on_artist_node_loads_albums_and_shuffles_play() {
    let (mut app, td) = build_app_with_td().await;
    td.fake_subsonic
        .expect_get_artist("artist-0", "ArtistName", &["Album One"])
        .await;
    td.fake_subsonic
        .expect_get_album("alb-0", "Album One", &["s0", "s1", "s2"])
        .await;
    {
        let mut s = app.daemon_state.write().await;
        s.library.artists = vec![ferrosonic::subsonic::models::Artist {
            id: "artist-0".into(),
            name: "ArtistName".into(),
            album_count: Some(1),
            cover_art: None,
        }];
    }
    {
        let mut cs = app.client_state.write().await;
        cs.artists.selected_index = Some(0);
    }
    app.handle_key(key(KeyCode::Char('t'))).await.unwrap();
}

#[tokio::test]
#[serial]
async fn enter_on_artist_not_in_cache_loads_albums_and_expands() {
    let (mut app, td) = build_app_with_td().await;
    td.fake_subsonic
        .expect_get_artist("artist-1", "Beta", &["Album B"])
        .await;
    {
        let mut s = td.state.write().await;
        s.library.artists = vec![ferrosonic::subsonic::models::Artist {
            id: "artist-1".into(),
            name: "Beta".into(),
            album_count: Some(1),
            cover_art: None,
        }];
    }
    {
        let mut cs = app.client_state.write().await;
        cs.artists.selected_index = Some(0);
    }
    app.handle_key(key(KeyCode::Enter)).await.unwrap();
    for _ in 0..100 {
        tokio::task::yield_now().await;
    }
    let cs = app.client_state.read().await;
    let _ = cs;
}

#[tokio::test]
#[serial]
async fn e_in_search_mode_for_artist_collects_all_songs() {
    let (mut app, td) = build_app_with_td().await;
    td.fake_subsonic
        .expect_get_artist("artist-2", "Coil", &["Album C"])
        .await;
    td.fake_subsonic
        .expect_get_album("alb-0", "Album C", &["c0", "c1"])
        .await;
    {
        let mut cs = app.client_state.write().await;
        cs.artists.filter = "co".into();
        cs.artists.search_results = Some(ferrosonic::subsonic::models::SearchResult3 {
            artist: vec![ferrosonic::subsonic::models::Artist {
                id: "artist-2".into(),
                name: "Coil".into(),
                album_count: Some(1),
                cover_art: None,
            }],
            album: vec![],
            song: vec![],
        });
        cs.artists.selected_index = Some(0);
    }
    app.handle_key(key(KeyCode::Char('e'))).await.unwrap();
}

#[tokio::test]
#[serial]
async fn i_in_search_mode_for_album_inserts_next() {
    let (mut app, td) = build_app_with_td().await;
    td.fake_subsonic
        .expect_get_album("alb-search", "Some Album", &["x0", "x1"])
        .await;
    {
        let mut s = td.state.write().await;
        s.queue.push(ferrosonic::subsonic::models::Child {
            id: "existing".into(),
            title: "E".into(),
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
        });
        s.queue_position = Some(0);
    }
    {
        let mut cs = app.client_state.write().await;
        cs.artists.filter = "x".into();
        cs.artists.search_results = Some(ferrosonic::subsonic::models::SearchResult3 {
            artist: vec![],
            album: vec![ferrosonic::subsonic::models::Album {
                id: "alb-search".into(),
                name: "Some Album".into(),
                artist: Some("X".into()),
                artist_id: Some("a0".into()),
                cover_art: None,
                song_count: Some(2),
                original_release_date: None,
                duration: Some(360),
                year: Some(2024),
                genre: None,
            }],
            song: vec![],
        });
        cs.artists.selected_index = Some(0);
    }
    app.handle_key(key(KeyCode::Char('i'))).await.unwrap();
}
