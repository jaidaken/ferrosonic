//! Search polish: debounce, configurable result limits, and recent-query
//! history recording and recall.

mod common;

use std::sync::Arc;
use std::time::Duration;

use common::RecordingClient;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ferrosonic::app::state::Page;
use ferrosonic::app::App;
use ferrosonic::config::Config;
use ferrosonic::ipc::client::DaemonClient;
use ferrosonic::ipc::protocol::DaemonRequest;
use serial_test::serial;

fn key(code: KeyCode) -> KeyEvent {
    let mut k = KeyEvent::new(code, KeyModifiers::NONE);
    k.kind = KeyEventKind::Press;
    k
}

struct Fx {
    app: App,
    client: Arc<RecordingClient>,
    _dir: tempfile::TempDir,
}

async fn build(debounce_ms: u32, limits: (u32, u32, u32)) -> Fx {
    let dir = common::tempdir();
    std::env::set_var("FERROSONIC_CONFIG_DIR", dir.path());
    let mut cfg = Config::new();
    cfg.search_debounce_ms = debounce_ms;
    cfg.search_artist_limit = limits.0;
    cfg.search_album_limit = limits.1;
    cfg.search_song_limit = limits.2;
    let client = RecordingClient::new();
    let client_dyn: Arc<dyn DaemonClient> = client.clone();
    let app = App::with_remote_client(client_dyn, cfg);
    app.client_state.write().await.page = Page::Library;
    Fx {
        app,
        client,
        _dir: dir,
    }
}

async fn search_requests(client: &RecordingClient) -> Vec<(u32, u32, u32, String)> {
    client
        .requests()
        .await
        .into_iter()
        .filter_map(|r| match r {
            DaemonRequest::Search {
                query,
                artist_count,
                album_count,
                song_count,
            } => Some((artist_count, album_count, song_count, query)),
            _ => None,
        })
        .collect()
}

#[tokio::test]
#[serial]
async fn search_uses_configured_result_limits() {
    let mut fx = build(0, (7, 8, 9)).await;
    fx.app.handle_key(key(KeyCode::Char('/'))).await.unwrap();
    fx.app.handle_key(key(KeyCode::Char('q'))).await.unwrap();
    tokio::time::sleep(Duration::from_millis(80)).await;

    let searches = search_requests(&fx.client).await;
    assert_eq!(searches.len(), 1, "one search issued");
    assert_eq!(
        searches[0],
        (7, 8, 9, "q".to_string()),
        "counts must come from config, not the old hardcoded caps"
    );
}

#[tokio::test]
#[serial]
async fn rapid_keystrokes_debounce_to_one_search() {
    let mut fx = build(300, (100, 100, 200)).await;
    fx.app.handle_key(key(KeyCode::Char('/'))).await.unwrap();
    for c in "cure".chars() {
        fx.app.handle_key(key(KeyCode::Char(c))).await.unwrap();
    }

    tokio::time::sleep(Duration::from_millis(60)).await;
    assert!(
        search_requests(&fx.client).await.is_empty(),
        "no request may fire before the debounce window elapses"
    );

    tokio::time::sleep(Duration::from_millis(350)).await;
    let searches = search_requests(&fx.client).await;
    assert_eq!(
        searches.len(),
        1,
        "four keystrokes must collapse to one request"
    );
    assert_eq!(searches[0].3, "cure", "the final query is the one searched");
}

#[tokio::test]
#[serial]
async fn successful_search_records_history_and_persists_it() {
    let mut fx = build(0, (100, 100, 200)).await;
    fx.app.handle_key(key(KeyCode::Char('/'))).await.unwrap();
    for c in "beach".chars() {
        fx.app.handle_key(key(KeyCode::Char(c))).await.unwrap();
    }
    tokio::time::sleep(Duration::from_millis(80)).await;

    assert_eq!(
        fx.app.client_state.read().await.search_history.first(),
        Some(&"beach".to_string()),
        "the issued query is remembered"
    );
    let path = fx._dir.path().join("search_history.json");
    let written = std::fs::read_to_string(&path).expect("history file written");
    assert!(written.contains("beach"), "history is persisted: {written}");
}

#[tokio::test]
#[serial]
async fn up_down_recall_recent_queries() {
    // A long debounce keeps the spawned search tasks from mutating history
    // while the recall cursor is exercised synchronously.
    let mut fx = build(60_000, (100, 100, 200)).await;
    {
        let mut cs = fx.app.client_state.write().await;
        cs.search_history = vec!["newer".into(), "older".into()];
        cs.artists.filter_active = true;
    }

    fx.app.handle_key(key(KeyCode::Up)).await.unwrap();
    assert_eq!(fx.app.client_state.read().await.artists.filter, "newer");
    fx.app.handle_key(key(KeyCode::Up)).await.unwrap();
    assert_eq!(fx.app.client_state.read().await.artists.filter, "older");
    // Down walks back toward the live query, clearing past the newest entry.
    fx.app.handle_key(key(KeyCode::Down)).await.unwrap();
    assert_eq!(fx.app.client_state.read().await.artists.filter, "newer");
    fx.app.handle_key(key(KeyCode::Down)).await.unwrap();
    assert!(
        fx.app.client_state.read().await.artists.filter.is_empty(),
        "down past the newest entry restores the empty live query"
    );
}
