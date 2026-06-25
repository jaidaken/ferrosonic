//! Library page key handlers: tree navigation, unified search, album-list view.

mod common;
use common::RecordingClient;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ferrosonic::app::page_state::LibraryView;
use ferrosonic::app::state::Page;
use ferrosonic::app::App;
use ferrosonic::config::Config;
use ferrosonic::ipc::client::DaemonClient;
use ferrosonic::ipc::protocol::{
    DaemonEvent, DaemonRequest, DaemonResponse, EnqueueMode, IpcError,
};
use ferrosonic::subsonic::models::{Album, Artist, Child, Playlist, SearchResult3};
use serial_test::serial;
use std::sync::Arc;

fn key(code: KeyCode) -> KeyEvent {
    let mut k = KeyEvent::new(code, KeyModifiers::NONE);
    k.kind = KeyEventKind::Press;
    k
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

fn album(id: &str, name: &str) -> Album {
    Album {
        id: id.into(),
        name: name.into(),
        artist: Some("Artist".into()),
        artist_id: Some("a".into()),
        cover_art: None,
        song_count: Some(2),
        original_release_date: None,
        duration: Some(200),
        year: Some(2000),
        genre: None,
    }
}

fn pl(id: &str) -> Playlist {
    Playlist {
        id: id.into(),
        name: id.into(),
        comment: None,
        owner: None,
        public: None,
        song_count: Some(1),
        duration: Some(60),
        cover_art: None,
    }
}

fn bare_song(id: &str, title: &str) -> Child {
    let mut s = Child::default();
    s.id = id.into();
    s.title = title.into();
    s
}

// === original: tree filter + unified search ===

#[tokio::test]
#[serial]
async fn slash_opens_search_then_types_into_filter() {
    let mut fx = build_app().await;
    fx.app.handle_key(key(KeyCode::Char('/'))).await.unwrap();
    fx.app.handle_key(key(KeyCode::Char('a'))).await.unwrap();
    fx.app.handle_key(key(KeyCode::Char('b'))).await.unwrap();
    let cs = fx.app.client_state.read().await;
    assert!(cs.artists.filter_active);
    assert_eq!(cs.artists.filter, "ab");
}

#[tokio::test]
#[serial]
async fn backspace_removes_last_filter_char() {
    let mut fx = build_app().await;
    fx.app.handle_key(key(KeyCode::Char('/'))).await.unwrap();
    fx.app.handle_key(key(KeyCode::Char('x'))).await.unwrap();
    fx.app.handle_key(key(KeyCode::Char('y'))).await.unwrap();
    fx.app.handle_key(key(KeyCode::Backspace)).await.unwrap();
    assert_eq!(fx.app.client_state.read().await.artists.filter, "x");
}

#[tokio::test]
#[serial]
async fn esc_closes_and_clears_filter() {
    let mut fx = build_app().await;
    fx.app.handle_key(key(KeyCode::Char('/'))).await.unwrap();
    fx.app.handle_key(key(KeyCode::Char('h'))).await.unwrap();
    fx.app.handle_key(key(KeyCode::Esc)).await.unwrap();
    let cs = fx.app.client_state.read().await;
    assert!(!cs.artists.filter_active);
    assert!(cs.artists.filter.is_empty());
}

#[tokio::test]
#[serial]
async fn down_lands_on_the_greyed_album_artist_now_selectable() {
    let mut fx = build_app().await;
    {
        let mut cs = fx.app.client_state.write().await;
        cs.artists.focus = 0;
        cs.artists.filter = "a".into();
        cs.artists.filter_active = false;
        cs.artists.search_results = Some(SearchResult3 {
            artist: vec![Artist {
                id: "a1".into(),
                name: "Matched Artist".into(),
                album_count: Some(1),
                cover_art: None,
            }],
            album: vec![Album {
                id: "alb1".into(),
                name: "An Album".into(),
                artist: Some("Other Artist".into()),
                artist_id: Some("a2".into()),
                cover_art: None,
                song_count: Some(1),
                original_release_date: None,
                duration: Some(100),
                year: Some(2000),
                genre: None,
            }],
            song: vec![],
        });
        cs.artists.selected_index = Some(0);
    }
    fx.app.handle_key(key(KeyCode::Down)).await.unwrap();
    assert_eq!(
        fx.app.client_state.read().await.artists.selected_index,
        Some(1),
        "Down lands on the greyed album-artist, which is now selectable"
    );
}

#[tokio::test]
#[serial]
async fn enter_closes_filter_but_keeps_content() {
    let mut fx = build_app().await;
    fx.app.handle_key(key(KeyCode::Char('/'))).await.unwrap();
    fx.app.handle_key(key(KeyCode::Char('q'))).await.unwrap();
    fx.app.handle_key(key(KeyCode::Enter)).await.unwrap();
    let cs = fx.app.client_state.read().await;
    assert!(!cs.artists.filter_active);
    assert_eq!(cs.artists.filter, "q");
}

#[tokio::test]
#[serial]
async fn slash_on_non_empty_filter_appends_literal_slash() {
    let mut fx = build_app().await;
    fx.app.handle_key(key(KeyCode::Char('/'))).await.unwrap();
    fx.app.handle_key(key(KeyCode::Char('x'))).await.unwrap();
    fx.app.handle_key(key(KeyCode::Char('/'))).await.unwrap();
    assert_eq!(fx.app.client_state.read().await.artists.filter, "x/");
}

#[tokio::test]
#[serial]
async fn library_page_is_active_after_f1() {
    let fx = build_app().await;
    assert_eq!(fx.app.client_state.read().await.page, Page::Library);
}

// === album-list view (handle_album_list_key) ===

async fn album_list_app(
    client: Arc<RecordingClient>,
    albums: Vec<Album>,
    sel: Option<usize>,
) -> App {
    let app = App::with_remote_client(client, Config::new());
    {
        let mut cs = app.client_state.write().await;
        cs.page = Page::Library;
        cs.artists.view = LibraryView::AlbumList;
        cs.artists.focus = 0;
        cs.artists.albums = albums;
        cs.artists.album_selected = sel;
    }
    app
}

// Returns songs for LoadAlbum so the pane-load path is observable.
struct AlbumLoadingClient {
    songs: Vec<Child>,
    event_tx: tokio::sync::broadcast::Sender<DaemonEvent>,
}
#[async_trait::async_trait]
impl DaemonClient for AlbumLoadingClient {
    async fn request(&self, req: DaemonRequest) -> Result<DaemonResponse, IpcError> {
        match req {
            DaemonRequest::LoadAlbum(_) => Ok(DaemonResponse::AlbumSongs(self.songs.clone())),
            _ => Ok(DaemonResponse::Ok),
        }
    }
    fn subscribe(&self) -> tokio::sync::broadcast::Receiver<DaemonEvent> {
        self.event_tx.subscribe()
    }
}

#[tokio::test]
#[serial]
async fn s_sort_selects_first_album_when_non_empty() {
    // 790 `delete !`
    let mut app = album_list_app(
        RecordingClient::new(),
        vec![album("a0", "Zed"), album("a1", "Abe")],
        None,
    )
    .await;
    app.handle_key(key(KeyCode::Char('s'))).await.unwrap();
    assert_eq!(
        app.client_state.read().await.artists.album_selected,
        Some(0)
    );
}

#[tokio::test]
#[serial]
async fn s_name_sort_drops_leading_quotes_in_album_key() {
    // 964 `delete !` in album_sort_key, via the observable 's' sort.
    let app = album_list_app(
        RecordingClient::new(),
        vec![album("a0", "\"Heroes\""), album("a1", "Abba")],
        Some(0),
    )
    .await;
    app.client_state.write().await.artists.album_sort =
        ferrosonic::app::page_state::AlbumSort::ReleaseDate; // 's' -> next() == Name
    let mut app = app;
    app.handle_key(key(KeyCode::Char('s'))).await.unwrap();
    assert_eq!(app.client_state.read().await.artists.albums[0].name, "Abba");
}

#[tokio::test]
#[serial]
async fn up_in_album_list_at_first_stays() {
    // 801 `> -> >=`
    let mut app = album_list_app(
        RecordingClient::new(),
        vec![album("a0", "A"), album("a1", "B")],
        Some(0),
    )
    .await;
    app.handle_key(key(KeyCode::Up)).await.unwrap();
    assert_eq!(
        app.client_state.read().await.artists.album_selected,
        Some(0)
    );
}

#[tokio::test]
#[serial]
async fn up_in_album_list_with_no_selection_inits_to_zero() {
    // 804 `delete !`
    let mut app = album_list_app(RecordingClient::new(), vec![album("a0", "A")], None).await;
    app.handle_key(key(KeyCode::Up)).await.unwrap();
    assert_eq!(
        app.client_state.read().await.artists.album_selected,
        Some(0)
    );
}

#[tokio::test]
#[serial]
async fn down_in_album_list_with_no_selection_inits_to_zero() {
    // 818 `delete !`
    let mut app = album_list_app(RecordingClient::new(), vec![album("a0", "A")], None).await;
    app.handle_key(key(KeyCode::Down)).await.unwrap();
    assert_eq!(
        app.client_state.read().await.artists.album_selected,
        Some(0)
    );
}

#[tokio::test]
#[serial]
async fn tab_focuses_song_pane_when_songs_present() {
    // 850 (delete Tab|Right arm) + 852 (`delete !`)
    let app = album_list_app(RecordingClient::new(), vec![album("a0", "A")], Some(0)).await;
    app.client_state.write().await.artists.songs = vec![common::song("s0", "S0")];
    let mut app = app;
    app.handle_key(key(KeyCode::Tab)).await.unwrap();
    assert_eq!(app.client_state.read().await.artists.focus, 1);
}

#[tokio::test]
#[serial]
async fn slash_opens_filter_in_album_list() {
    // 859 (delete '/' arm)
    let mut app = album_list_app(RecordingClient::new(), vec![album("a0", "A")], Some(0)).await;
    app.handle_key(key(KeyCode::Char('/'))).await.unwrap();
    assert!(app.client_state.read().await.artists.filter_active);
}

#[tokio::test]
#[serial]
async fn enter_in_album_list_plays_the_pane_songs() {
    // 824 (delete Enter arm)
    let client = RecordingClient::new();
    let app = album_list_app(client.clone(), vec![album("a0", "A")], Some(0)).await;
    app.client_state.write().await.artists.songs =
        vec![common::song("s0", "S0"), common::song("s1", "S1")];
    let mut app = app;
    app.handle_key(key(KeyCode::Enter)).await.unwrap();
    assert!(client
        .requests()
        .await
        .iter()
        .any(|r| matches!(r, DaemonRequest::EnqueueSongs { .. })));
}

#[tokio::test]
#[serial]
async fn selecting_album_loads_its_songs_into_pane() {
    // 908 (fn -> ()) + 918 (`delete !`)
    let (event_tx, _) = tokio::sync::broadcast::channel(16);
    let client = Arc::new(AlbumLoadingClient {
        songs: vec![common::song("s0", "S0"), common::song("s1", "S1")],
        event_tx,
    });
    let mut app = App::with_remote_client(client, Config::new());
    {
        let mut cs = app.client_state.write().await;
        cs.page = Page::Library;
        cs.artists.view = LibraryView::AlbumList;
        cs.artists.focus = 0;
        cs.artists.albums = vec![album("a0", "A")];
        cs.artists.album_selected = None;
    }
    app.handle_key(key(KeyCode::Down)).await.unwrap();
    let cs = app.client_state.read().await;
    assert_eq!(cs.artists.songs.len(), 2);
    assert_eq!(cs.artists.selected_song, Some(0));
}

// === handle_library_key song pane (focus 1) ===

async fn song_pane_app(client: Arc<RecordingClient>, focus: usize) -> App {
    let app = App::with_remote_client(client, Config::new());
    {
        let mut cs = app.client_state.write().await;
        cs.page = Page::Library;
        cs.artists.focus = focus;
        cs.artists.songs = vec![common::song("s0", "S0"), common::song("s1", "S1")];
        cs.artists.selected_song = Some(0);
    }
    app
}

#[tokio::test]
#[serial]
async fn enter_in_song_pane_out_of_range_does_not_play() {
    // 401 `< -> <=`
    let client = RecordingClient::new();
    let app = song_pane_app(client.clone(), 1).await;
    app.client_state.write().await.artists.selected_song = Some(5);
    let mut app = app;
    app.handle_key(key(KeyCode::Enter)).await.unwrap();
    assert!(!client
        .requests()
        .await
        .iter()
        .any(|r| matches!(r, DaemonRequest::EnqueueSongs { .. })));
}

#[tokio::test]
#[serial]
async fn backspace_returns_to_tree_from_song_pane_only() {
    // 422 (focus==1 guard)
    let mut app = song_pane_app(RecordingClient::new(), 1).await;
    app.handle_key(key(KeyCode::Backspace)).await.unwrap();
    assert_eq!(app.client_state.read().await.artists.focus, 0);
}

#[tokio::test]
#[serial]
async fn m_stars_in_song_pane_only() {
    // 559 (focus==1 guard, both positions)
    let client = RecordingClient::new();
    let mut app = song_pane_app(client.clone(), 1).await;
    app.handle_key(key(KeyCode::Char('m'))).await.unwrap();
    assert!(client
        .requests()
        .await
        .iter()
        .any(|r| matches!(r, DaemonRequest::ToggleStarSong(_))));
    let client = RecordingClient::new();
    let mut app = song_pane_app(client.clone(), 0).await;
    app.handle_key(key(KeyCode::Char('m'))).await.unwrap();
    assert!(!client
        .requests()
        .await
        .iter()
        .any(|r| matches!(r, DaemonRequest::ToggleStarSong(_))));
}

#[tokio::test]
#[serial]
async fn a_opens_picker_in_song_pane_only() {
    // 573 (focus==1 guard)
    let app = song_pane_app(RecordingClient::new(), 1).await;
    app.daemon_state.write().await.library.playlists = vec![pl("p0")];
    let mut app = app;
    app.handle_key(key(KeyCode::Char('a'))).await.unwrap();
    assert!(app.client_state.read().await.playlist_picker.active);
    let mut app = song_pane_app(RecordingClient::new(), 0).await;
    app.handle_key(key(KeyCode::Char('a'))).await.unwrap();
    assert!(!app.client_state.read().await.playlist_picker.active);
}

#[tokio::test]
#[serial]
async fn m_does_not_star_when_focus0_and_not_filtering() {
    // 585 guard `-> true`: the filter-mode 'm' arm must not fire with no filter.
    let client = RecordingClient::new();
    let app = App::with_remote_client(client.clone(), Config::new());
    {
        let mut cs = app.client_state.write().await;
        cs.page = Page::Library;
        cs.artists.focus = 0;
        cs.artists.filter.clear();
        cs.artists.search_results = None;
        cs.artists.selected_index = Some(0);
    }
    let mut app = app;
    app.handle_key(key(KeyCode::Char('m'))).await.unwrap();
    assert!(!client
        .requests()
        .await
        .iter()
        .any(|r| matches!(r, DaemonRequest::ToggleStarSong(_))));
}

// === filter-mode tree actions (focus 0, non-empty filter, search results) ===

async fn filter_song_app(client: Arc<RecordingClient>) -> App {
    let app = App::with_remote_client(client, Config::new());
    {
        let mut cs = app.client_state.write().await;
        cs.page = Page::Library;
        cs.artists.focus = 0;
        cs.artists.filter = "s".into();
        cs.artists.filter_active = false;
        cs.artists.search_results = Some(SearchResult3 {
            artist: vec![],
            album: vec![],
            song: vec![bare_song("s0", "Song Zero")],
        });
        cs.artists.selected_index = Some(0);
    }
    app
}

#[tokio::test]
#[serial]
async fn m_stars_the_selected_search_song_in_filter_mode() {
    // 585 (guard) + 596 (Song match arm)
    let client = RecordingClient::new();
    let mut app = filter_song_app(client.clone()).await;
    app.handle_key(key(KeyCode::Char('m'))).await.unwrap();
    assert!(client
        .requests()
        .await
        .iter()
        .any(|r| matches!(r, DaemonRequest::ToggleStarSong(id) if id == "s0")));
}

#[tokio::test]
#[serial]
async fn e_appends_selected_search_item_in_filter_mode() {
    // 445 (`&& -> ||`): the 'e' filter-append branch needs filter AND results.
    let client = RecordingClient::new();
    let mut app = filter_song_app(client.clone()).await;
    app.handle_key(key(KeyCode::Char('e'))).await.unwrap();
    assert!(client
        .requests()
        .await
        .iter()
        .any(|r| matches!(r, DaemonRequest::EnqueueSongs { mode, .. } if matches!(mode, EnqueueMode::Append))));
}

#[tokio::test]
#[serial]
async fn i_inserts_selected_search_item_in_filter_mode() {
    // 515 (`&& -> ||`): the 'i' filter-insert branch needs filter AND results.
    let client = RecordingClient::new();
    let mut app = filter_song_app(client.clone()).await;
    app.handle_key(key(KeyCode::Char('i'))).await.unwrap();
    assert!(client
        .requests()
        .await
        .iter()
        .any(|r| matches!(r, DaemonRequest::EnqueueSongs { .. })));
}

#[tokio::test]
#[serial]
async fn t_shuffles_the_selected_search_song() {
    // 131 ('t' focus==0 guard)
    let client = RecordingClient::new();
    let mut app = filter_song_app(client.clone()).await;
    app.handle_key(key(KeyCode::Char('t'))).await.unwrap();
    assert!(client
        .requests()
        .await
        .iter()
        .any(|r| matches!(r, DaemonRequest::EnqueueSongs { mode, .. } if matches!(mode, EnqueueMode::Replace { .. }))));
}

// === view toggle (handle_library_view_toggle, spawned load) ===

struct ViewLoadClient {
    albums: Vec<Album>,
    songs: Vec<Child>,
    event_tx: tokio::sync::broadcast::Sender<DaemonEvent>,
}
#[async_trait::async_trait]
impl DaemonClient for ViewLoadClient {
    async fn request(&self, req: DaemonRequest) -> Result<DaemonResponse, IpcError> {
        match req {
            DaemonRequest::LoadAllAlbums => Ok(DaemonResponse::AllAlbums(self.albums.clone())),
            DaemonRequest::LoadAlbum(_) => Ok(DaemonResponse::AlbumSongs(self.songs.clone())),
            _ => Ok(DaemonResponse::Ok),
        }
    }
    fn subscribe(&self) -> tokio::sync::broadcast::Receiver<DaemonEvent> {
        self.event_tx.subscribe()
    }
}

#[tokio::test]
#[serial]
async fn v_loads_albums_into_the_album_list_view() {
    // 702 (need_load &&), 713 (AllAlbums arm), 720/729 (selection `delete !`)
    let (event_tx, _) = tokio::sync::broadcast::channel(16);
    let client = Arc::new(ViewLoadClient {
        albums: vec![album("a0", "A"), album("a1", "B")],
        songs: vec![common::song("s0", "S0")],
        event_tx,
    });
    let mut app = App::with_remote_client(client, Config::new());
    {
        let mut cs = app.client_state.write().await;
        cs.page = Page::Library;
        cs.artists.view = LibraryView::ArtistTree;
        cs.artists.albums.clear();
    }
    app.handle_key(key(KeyCode::Char('v'))).await.unwrap();
    for _ in 0..50 {
        tokio::task::yield_now().await; // let the spawned load run
    }
    let cs = app.client_state.read().await;
    assert_eq!(cs.artists.view, LibraryView::AlbumList);
    assert_eq!(
        cs.artists.albums.len(),
        2,
        "albums loaded via LoadAllAlbums"
    );
    assert_eq!(cs.artists.album_selected, Some(0));
    assert_eq!(cs.artists.selected_song, Some(0));
}
