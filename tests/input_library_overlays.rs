//! Library overlay methods: handle_library_view_toggle (async album fetch),
//! handle_library_filter_key (async search), handle_library_folder_cycle. The
//! view-toggle and filter spawn background tasks, so tests settle the spawn
//! (bounded yield loop) before asserting.

mod common;

use async_trait::async_trait;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ferrosonic::app::page_state::LibraryView;
use ferrosonic::app::state::Page;
use ferrosonic::app::App;
use ferrosonic::config::Config;
use ferrosonic::ipc::client::DaemonClient;
use ferrosonic::ipc::protocol::{DaemonEvent, DaemonRequest, DaemonResponse, IpcError};
use ferrosonic::subsonic::models::{Album, Child, MusicFolder, SearchResult3};
use serial_test::serial;
use std::sync::Arc;
use tokio::sync::broadcast;

fn key(code: KeyCode) -> KeyEvent {
    let mut k = KeyEvent::new(code, KeyModifiers::NONE);
    k.kind = KeyEventKind::Press;
    k
}

fn album(id: &str, name: &str) -> Album {
    Album {
        id: id.into(),
        name: name.into(),
        artist: Some("Ar".into()),
        artist_id: Some("a".into()),
        cover_art: None,
        song_count: Some(1),
        original_release_date: None,
        duration: Some(100),
        year: Some(2000),
        genre: None,
    }
}

// Client that answers the overlay async fetches with canned catalog data.
struct OverlayClient {
    albums: Vec<Album>,
    album_songs: Vec<Child>,
    search: Option<SearchResult3>,
    event_tx: broadcast::Sender<DaemonEvent>,
}

impl OverlayClient {
    fn new(
        albums: Vec<Album>,
        album_songs: Vec<Child>,
        search: Option<SearchResult3>,
    ) -> Arc<Self> {
        let (event_tx, _) = broadcast::channel(16);
        Arc::new(Self {
            albums,
            album_songs,
            search,
            event_tx,
        })
    }
}

#[async_trait]
impl DaemonClient for OverlayClient {
    async fn request(&self, req: DaemonRequest) -> Result<DaemonResponse, IpcError> {
        match req {
            DaemonRequest::LoadAllAlbums => Ok(DaemonResponse::AllAlbums(self.albums.clone())),
            DaemonRequest::LoadAlbum(_) => Ok(DaemonResponse::AlbumSongs(self.album_songs.clone())),
            DaemonRequest::Search { .. } => match &self.search {
                Some(s) => Ok(DaemonResponse::SearchResults(s.clone())),
                None => Ok(DaemonResponse::Ok),
            },
            _ => Ok(DaemonResponse::Ok),
        }
    }
    fn subscribe(&self) -> broadcast::Receiver<DaemonEvent> {
        self.event_tx.subscribe()
    }
}

fn app_with(client: Arc<OverlayClient>) -> App {
    App::with_remote_client(client, Config::new())
}

// (713/720/729 - the LoadAllAlbums fetch path - are already killed by the
// committed `v_loads_albums_into_the_album_list_view` test in input_library_keys.)

// 702 (`&&` -> `||` in need_load): 'v' with albums already loaded must NOT refetch
// (the mutant fetches even when the cache is warm, clobbering the list).
#[tokio::test]
#[serial]
async fn view_toggle_with_albums_loaded_does_not_refetch() {
    let client = OverlayClient::new(vec![album("FETCHED", "X")], vec![], None);
    let mut app = app_with(client);
    {
        let mut cs = app.client_state.write().await;
        cs.page = Page::Library;
        cs.artists.view = LibraryView::ArtistTree;
        cs.artists.albums = vec![album("preset0", "P0"), album("preset1", "P1")];
    }
    app.handle_key(key(KeyCode::Char('v'))).await.unwrap();
    for _ in 0..400 {
        tokio::task::yield_now().await;
    }
    let cs = app.client_state.read().await;
    assert_eq!(
        cs.artists.albums.len(),
        2,
        "albums must stay the preset two"
    );
    assert_eq!(
        cs.artists.albums[0].id, "preset0",
        "702: must not refetch when albums are already loaded"
    );
}

// 681 (`==` -> `!=` stale-gen check): a search result issued for the current gen
// must be applied to search_results.
#[tokio::test]
#[serial]
async fn filter_search_result_for_current_gen_is_applied() {
    let search = SearchResult3 {
        artist: vec![],
        album: vec![],
        song: vec![common::song("hit", "Hit Song")],
    };
    let client = OverlayClient::new(vec![], vec![], Some(search));
    let mut app = app_with(client);
    {
        let mut cs = app.client_state.write().await;
        cs.page = Page::Library;
        cs.artists.filter_active = true;
        cs.artists.filter.clear();
        cs.artists.search_results = None;
    }
    app.handle_key(key(KeyCode::Char('x'))).await.unwrap();
    let mut applied = false;
    for _ in 0..2000 {
        if app
            .client_state
            .read()
            .await
            .artists
            .search_results
            .is_some()
        {
            applied = true;
            break;
        }
        tokio::task::yield_now().await;
    }
    assert!(
        applied,
        "681: a search result for the current gen must be applied (== gen)"
    );
}

// 764 (`==` -> `!=` folder label lookup): cycling the active library notifies with
// the target folder's name, not some other folder's.
#[tokio::test]
#[serial]
async fn folder_cycle_label_names_the_target_folder() {
    let app = app_with(OverlayClient::new(vec![], vec![], None));
    {
        let mut ds = app.daemon_state.write().await;
        ds.library.music_folders = vec![
            MusicFolder {
                id: 1,
                name: "Jazz".into(),
            },
            MusicFolder {
                id: 2,
                name: "Rock".into(),
            },
        ];
        ds.config.music_folder_id = None; // currently "All" -> next is folder id 1
    }
    app.client_state.write().await.page = Page::Library;
    let mut app = app;
    app.handle_key(key(KeyCode::Char('f'))).await.unwrap();
    let cs = app.client_state.read().await;
    let label = cs.notification.as_ref().map(|n| n.message.as_str());
    assert_eq!(
        label,
        Some("Library: Jazz"),
        "764: folder label must name folder id 1 (Jazz)"
    );
}
