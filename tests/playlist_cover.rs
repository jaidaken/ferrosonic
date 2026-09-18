//! Playlist cover-art fetch trigger: the Playlists page fetches the selected
//! playlist's art on the tick path, only when cover art is enabled and the
//! playlist actually has a cover id.

mod common;

use common::RecordingClient;
use ferrosonic::app::state::Page;
use ferrosonic::app::App;
use ferrosonic::config::Config;
use ferrosonic::ipc::DaemonRequest;
use ferrosonic::subsonic::models::Playlist;

fn playlist_with_cover(id: &str, cover: Option<&str>) -> Playlist {
    Playlist {
        id: id.to_string(),
        name: id.to_string(),
        owner: None,
        song_count: Some(3),
        duration: Some(180),
        cover_art: cover.map(str::to_owned),
        public: None,
        comment: None,
    }
}

async fn app_with_playlist(client: std::sync::Arc<RecordingClient>) -> App {
    let mut cfg = Config::new();
    cfg.cover_art = true;
    let app = App::with_remote_client(client, cfg);
    {
        let mut ds = app.daemon_state.write().await;
        ds.library.playlists = vec![playlist_with_cover("p0", Some("pl-cover"))];
    }
    {
        let mut cs = app.client_state.write().await;
        cs.page = Page::Playlists;
        cs.playlists.selected_playlist = Some(0);
    }
    app
}

#[tokio::test]
async fn selecting_a_playlist_with_art_fetches_its_cover() {
    let client = RecordingClient::new();
    let mut app = app_with_playlist(client.clone()).await;

    app.tick_post().await;
    // The fetch is spawned; give it a chance to run.
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let recorded = client.requests().await;
    assert!(
        recorded.iter().any(|r| matches!(
            r,
            DaemonRequest::FetchCoverArt { id, .. } if id == "pl-cover"
        )),
        "expected a FetchCoverArt for the selected playlist; got {recorded:?}"
    );
}

#[tokio::test]
async fn playlist_without_art_does_not_fetch() {
    let client = RecordingClient::new();
    let mut cfg = Config::new();
    cfg.cover_art = true;
    let mut app = App::with_remote_client(client.clone(), cfg);
    {
        let mut ds = app.daemon_state.write().await;
        ds.library.playlists = vec![playlist_with_cover("p0", None)];
    }
    {
        let mut cs = app.client_state.write().await;
        cs.page = Page::Playlists;
        cs.playlists.selected_playlist = Some(0);
    }

    app.tick_post().await;
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let recorded = client.requests().await;
    assert!(
        !recorded
            .iter()
            .any(|r| matches!(r, DaemonRequest::FetchCoverArt { .. })),
        "a playlist with no coverArt must not fetch; got {recorded:?}"
    );
}

#[tokio::test]
async fn other_pages_do_not_fetch_playlist_art() {
    let client = RecordingClient::new();
    let mut app = app_with_playlist(client.clone()).await;
    app.client_state.write().await.page = Page::Library;

    app.tick_post().await;
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let recorded = client.requests().await;
    assert!(
        !recorded
            .iter()
            .any(|r| matches!(r, DaemonRequest::FetchCoverArt { .. })),
        "playlist art must only fetch while the Playlists page is active; got {recorded:?}"
    );
}
