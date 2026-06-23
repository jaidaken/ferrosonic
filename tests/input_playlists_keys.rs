//! handle_playlists_key: focus-pane match guards fire only in the right pane, and
//! the playlist/song nav stays in bounds. Closes the cargo-mutants survivors
//! (focus == N guards + Up/Down arithmetic).

mod common;
use common::{song, RecordingClient};
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ferrosonic::app::state::Page;
use ferrosonic::app::App;
use ferrosonic::config::Config;
use ferrosonic::ipc::protocol::DaemonRequest;
use ferrosonic::subsonic::models::Playlist;
use serial_test::serial;
use std::sync::Arc;

fn key(code: KeyCode) -> KeyEvent {
    let mut k = KeyEvent::new(code, KeyModifiers::NONE);
    k.kind = KeyEventKind::Press;
    k
}

fn playlist(id: &str) -> Playlist {
    Playlist {
        id: id.into(),
        name: id.into(),
        comment: None,
        owner: None,
        public: None,
        song_count: Some(2),
        duration: Some(120),
        cover_art: None,
    }
}

// App on the Playlists page with one playlist selected and a two-song pane.
async fn app_on_playlists(client: Arc<RecordingClient>, focus: usize) -> App {
    let app = App::with_remote_client(client, Config::new());
    {
        let mut ds = app.daemon_state.write().await;
        ds.library.playlists = vec![playlist("p0")];
    }
    {
        let mut cs = app.client_state.write().await;
        cs.page = Page::Playlists;
        cs.playlists.focus = focus;
        cs.playlists.selected_playlist = Some(0);
        cs.playlists.songs = vec![song("s0", "S0"), song("s1", "S1")];
        cs.playlists.selected_song = Some(0);
    }
    app
}

fn has(reqs: &[DaemonRequest], pred: impl Fn(&DaemonRequest) -> bool) -> bool {
    reqs.iter().any(pred)
}

// --- focus-pane match guards: fire in the right pane, no-op in the wrong one ---

#[tokio::test]
#[serial]
async fn m_stars_in_song_pane_only() {
    // focus == 1: stars.
    let c = RecordingClient::new();
    let mut app = app_on_playlists(c.clone(), 1).await;
    app.handle_key(key(KeyCode::Char('m'))).await.unwrap();
    assert!(has(&c.requests().await, |r| matches!(
        r,
        DaemonRequest::ToggleStarSong(_)
    )));
    // focus == 0: must not star (kills `== 1 -> true`).
    let c = RecordingClient::new();
    let mut app = app_on_playlists(c.clone(), 0).await;
    app.handle_key(key(KeyCode::Char('m'))).await.unwrap();
    assert!(!has(&c.requests().await, |r| matches!(
        r,
        DaemonRequest::ToggleStarSong(_)
    )));
}

#[tokio::test]
#[serial]
async fn d_removes_in_song_pane_only() {
    let c = RecordingClient::new();
    let mut app = app_on_playlists(c.clone(), 1).await;
    app.handle_key(key(KeyCode::Char('d'))).await.unwrap();
    assert!(has(&c.requests().await, |r| matches!(
        r,
        DaemonRequest::RemovePlaylistSong { .. }
    )));
    let c = RecordingClient::new();
    let mut app = app_on_playlists(c.clone(), 0).await;
    app.handle_key(key(KeyCode::Char('d'))).await.unwrap();
    assert!(!has(&c.requests().await, |r| matches!(
        r,
        DaemonRequest::RemovePlaylistSong { .. }
    )));
}

#[tokio::test]
#[serial]
async fn shift_j_and_k_reorder_in_song_pane_only() {
    for code in [KeyCode::Char('J'), KeyCode::Char('K')] {
        let c = RecordingClient::new();
        let mut app = app_on_playlists(c.clone(), 1).await;
        {
            // Middle of a 3-song pane so both J (down) and K (up) are in bounds.
            let mut cs = app.client_state.write().await;
            cs.playlists.songs = vec![song("s0", "S0"), song("s1", "S1"), song("s2", "S2")];
            cs.playlists.selected_song = Some(1);
        }
        app.handle_key(key(code)).await.unwrap();
        assert!(
            has(&c.requests().await, |r| matches!(
                r,
                DaemonRequest::ReorderPlaylist { .. }
            )),
            "{code:?} in song pane must reorder"
        );
        let c = RecordingClient::new();
        let mut app = app_on_playlists(c.clone(), 0).await;
        app.handle_key(key(code)).await.unwrap();
        assert!(
            !has(&c.requests().await, |r| matches!(
                r,
                DaemonRequest::ReorderPlaylist { .. }
            )),
            "{code:?} in playlist pane must not reorder"
        );
    }
}

#[tokio::test]
#[serial]
async fn a_opens_picker_in_song_pane_only() {
    let c = RecordingClient::new();
    let mut app = app_on_playlists(c.clone(), 1).await;
    app.handle_key(key(KeyCode::Char('a'))).await.unwrap();
    assert!(app.client_state.read().await.playlist_picker.active);
    let c = RecordingClient::new();
    let mut app = app_on_playlists(c.clone(), 0).await;
    app.handle_key(key(KeyCode::Char('a'))).await.unwrap();
    assert!(!app.client_state.read().await.playlist_picker.active);
}

#[tokio::test]
#[serial]
async fn shift_r_renames_in_playlist_pane_only() {
    let c = RecordingClient::new();
    let mut app = app_on_playlists(c.clone(), 0).await;
    app.handle_key(key(KeyCode::Char('R'))).await.unwrap();
    assert!(app.client_state.read().await.playlists.renaming);
    let c = RecordingClient::new();
    let mut app = app_on_playlists(c.clone(), 1).await;
    app.handle_key(key(KeyCode::Char('R'))).await.unwrap();
    assert!(!app.client_state.read().await.playlists.renaming);
}

#[tokio::test]
#[serial]
async fn shift_d_confirms_delete_in_playlist_pane_only() {
    let c = RecordingClient::new();
    let mut app = app_on_playlists(c.clone(), 0).await;
    app.handle_key(key(KeyCode::Char('D'))).await.unwrap();
    assert!(app.client_state.read().await.playlists.confirming_delete);
    let c = RecordingClient::new();
    let mut app = app_on_playlists(c.clone(), 1).await;
    app.handle_key(key(KeyCode::Char('D'))).await.unwrap();
    assert!(!app.client_state.read().await.playlists.confirming_delete);
}

// --- nav bounds (Up/Down arithmetic) ---

#[tokio::test]
#[serial]
async fn up_in_playlist_pane_with_no_selection_inits_to_zero() {
    let c = RecordingClient::new();
    let app = app_on_playlists(c, 0).await;
    app.client_state.write().await.playlists.selected_playlist = None;
    let mut app = app;
    app.handle_key(key(KeyCode::Up)).await.unwrap();
    assert_eq!(
        app.client_state.read().await.playlists.selected_playlist,
        Some(0)
    );
}

#[tokio::test]
#[serial]
async fn up_in_song_pane_at_first_stays() {
    let c = RecordingClient::new();
    let mut app = app_on_playlists(c, 1).await;
    app.client_state.write().await.playlists.selected_song = Some(0);
    app.handle_key(key(KeyCode::Up)).await.unwrap();
    assert_eq!(
        app.client_state.read().await.playlists.selected_song,
        Some(0)
    );
}

#[tokio::test]
#[serial]
async fn up_in_song_pane_with_no_selection_inits_to_zero() {
    let c = RecordingClient::new();
    let mut app = app_on_playlists(c, 1).await;
    app.client_state.write().await.playlists.selected_song = None;
    app.handle_key(key(KeyCode::Up)).await.unwrap();
    assert_eq!(
        app.client_state.read().await.playlists.selected_song,
        Some(0)
    );
}

#[tokio::test]
#[serial]
async fn down_in_playlist_pane_at_last_stays() {
    let c = RecordingClient::new();
    let mut app = app_on_playlists(c, 0).await;
    app.client_state.write().await.playlists.selected_playlist = Some(0); // len 1 -> max 0
    app.handle_key(key(KeyCode::Down)).await.unwrap();
    assert_eq!(
        app.client_state.read().await.playlists.selected_playlist,
        Some(0)
    );
}

#[tokio::test]
#[serial]
async fn down_in_song_pane_advances_then_clamps() {
    let c = RecordingClient::new();
    let mut app = app_on_playlists(c, 1).await;
    app.client_state.write().await.playlists.selected_song = Some(0);
    app.handle_key(key(KeyCode::Down)).await.unwrap();
    assert_eq!(
        app.client_state.read().await.playlists.selected_song,
        Some(1)
    );
    app.handle_key(key(KeyCode::Down)).await.unwrap();
    assert_eq!(
        app.client_state.read().await.playlists.selected_song,
        Some(1)
    );
}

// Closes the 107 `count > 0` seam: Entering a playlist with songs selects the
// first one. Needs a client that returns songs for LoadPlaylist.
struct LoadingClient {
    songs: Vec<ferrosonic::subsonic::models::Child>,
    event_tx: tokio::sync::broadcast::Sender<ferrosonic::ipc::protocol::DaemonEvent>,
}

#[async_trait::async_trait]
impl ferrosonic::ipc::client::DaemonClient for LoadingClient {
    async fn request(
        &self,
        req: DaemonRequest,
    ) -> Result<ferrosonic::ipc::protocol::DaemonResponse, ferrosonic::ipc::protocol::IpcError>
    {
        match req {
            DaemonRequest::LoadPlaylist(_) => Ok(
                ferrosonic::ipc::protocol::DaemonResponse::PlaylistSongs(self.songs.clone()),
            ),
            _ => Ok(ferrosonic::ipc::protocol::DaemonResponse::Ok),
        }
    }
    fn subscribe(
        &self,
    ) -> tokio::sync::broadcast::Receiver<ferrosonic::ipc::protocol::DaemonEvent> {
        self.event_tx.subscribe()
    }
}

#[tokio::test]
#[serial]
async fn enter_on_playlist_with_songs_selects_first_song() {
    let (event_tx, _) = tokio::sync::broadcast::channel(16);
    let client = Arc::new(LoadingClient {
        songs: vec![song("s0", "S0"), song("s1", "S1")],
        event_tx,
    });
    let mut app = App::with_remote_client(client, Config::new());
    {
        let mut ds = app.daemon_state.write().await;
        ds.library.playlists = vec![playlist("p0")];
    }
    {
        let mut cs = app.client_state.write().await;
        cs.page = Page::Playlists;
        cs.playlists.focus = 0;
        cs.playlists.selected_playlist = Some(0);
    }
    app.handle_key(key(KeyCode::Enter)).await.unwrap();
    assert_eq!(
        app.client_state.read().await.playlists.selected_song,
        Some(0),
        "Entering a non-empty playlist must select its first song"
    );
}
