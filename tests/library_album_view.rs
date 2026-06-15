//! Library album-list view: the `v` toggle, `s` sort cycle, and album-list
//! cursor navigation.

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ferrosonic::app::page_state::{AlbumSort, LibraryView};
use ferrosonic::app::App;
use ferrosonic::config::Config;
use ferrosonic::subsonic::models::{Album, ItemDate};
use serial_test::serial;

fn key(code: KeyCode) -> KeyEvent {
    let mut k = KeyEvent::new(code, KeyModifiers::NONE);
    k.kind = KeyEventKind::Press;
    k
}

fn album(id: &str, name: &str, year: i32) -> Album {
    Album {
        id: id.into(),
        name: name.into(),
        artist: Some("Artist".into()),
        artist_id: Some("a0".into()),
        cover_art: None,
        song_count: Some(1),
        duration: Some(100),
        year: Some(year + 5),
        original_release_date: Some(ItemDate {
            year: Some(year),
            month: None,
            day: None,
        }),
        genre: None,
    }
}

async fn build_app() -> (App, tempfile::TempDir) {
    let tempdir = tempfile::tempdir().unwrap();
    std::env::set_var("FERROSONIC_CONFIG_DIR", tempdir.path());
    let mut config = Config::new();
    config.daemon = false;
    let mut app = App::new(config);
    app.handle_key(key(KeyCode::F(1))).await.unwrap();
    (app, tempdir)
}

#[tokio::test]
#[serial]
async fn v_toggles_between_artist_tree_and_album_list() {
    let (mut app, _t) = build_app().await;
    assert_eq!(
        app.client_state.read().await.artists.view,
        LibraryView::ArtistTree,
        "default view is the artist tree"
    );

    app.handle_key(key(KeyCode::Char('v'))).await.unwrap();
    assert_eq!(
        app.client_state.read().await.artists.view,
        LibraryView::AlbumList
    );

    app.handle_key(key(KeyCode::Char('v'))).await.unwrap();
    assert_eq!(
        app.client_state.read().await.artists.view,
        LibraryView::ArtistTree
    );
}

#[tokio::test]
#[serial]
async fn s_cycles_sort_and_reorders_the_album_list() {
    let (mut app, _t) = build_app().await;
    {
        let mut cs = app.client_state.write().await;
        cs.artists.view = LibraryView::AlbumList;
        cs.artists.focus = 0;
        // Name order C, A, B; release years 1990, 2010, 1980.
        cs.artists.albums = vec![album("c", "C", 1990), album("a", "A", 2010), album("b", "B", 1980)];
        cs.artists.album_selected = Some(0);
    }

    app.handle_key(key(KeyCode::Char('s'))).await.unwrap();
    {
        let cs = app.client_state.read().await;
        assert_eq!(cs.artists.album_sort, AlbumSort::ReleaseDate);
        let names: Vec<&str> = cs.artists.albums.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(names, vec!["B", "C", "A"], "sorted by original release year, oldest first");
    }

    app.handle_key(key(KeyCode::Char('s'))).await.unwrap();
    {
        let cs = app.client_state.read().await;
        assert_eq!(cs.artists.album_sort, AlbumSort::Name);
        let names: Vec<&str> = cs.artists.albums.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(names, vec!["A", "B", "C"], "sorted alphabetically");
    }
}

#[tokio::test]
#[serial]
async fn album_list_down_and_up_move_the_selection() {
    let (mut app, _t) = build_app().await;
    {
        let mut cs = app.client_state.write().await;
        cs.artists.view = LibraryView::AlbumList;
        cs.artists.focus = 0;
        cs.artists.albums = vec![album("a", "A", 2000), album("b", "B", 2001), album("c", "C", 2002)];
        cs.artists.album_selected = Some(0);
    }

    app.handle_key(key(KeyCode::Down)).await.unwrap();
    assert_eq!(app.client_state.read().await.artists.album_selected, Some(1));

    app.handle_key(key(KeyCode::Down)).await.unwrap();
    app.handle_key(key(KeyCode::Down)).await.unwrap();
    assert_eq!(
        app.client_state.read().await.artists.album_selected,
        Some(2),
        "Down clamps at the last album"
    );

    app.handle_key(key(KeyCode::Up)).await.unwrap();
    assert_eq!(app.client_state.read().await.artists.album_selected, Some(1));
}

#[tokio::test]
#[serial]
async fn name_sort_ignores_leading_punctuation() {
    let (mut app, _t) = build_app().await;
    {
        let mut cs = app.client_state.write().await;
        cs.artists.view = LibraryView::AlbumList;
        cs.artists.focus = 0;
        cs.artists.albums = vec![
            album("h", "\"Heroes\"", 1977),
            album("c", "A Crow Looked At Me", 2017),
        ];
        // Start on ReleaseDate so one 's' press cycles to Name and re-sorts.
        cs.artists.album_sort = AlbumSort::ReleaseDate;
        cs.artists.album_selected = Some(0);
    }

    app.handle_key(key(KeyCode::Char('s'))).await.unwrap();

    let cs = app.client_state.read().await;
    assert_eq!(cs.artists.album_sort, AlbumSort::Name);
    let names: Vec<&str> = cs.artists.albums.iter().map(|a| a.name.as_str()).collect();
    assert_eq!(
        names,
        vec!["A Crow Looked At Me", "\"Heroes\""],
        "the leading quote is ignored, so A sorts before H"
    );
}
