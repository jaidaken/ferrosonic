//! Lyrics overlay behavior: background loading, caching, errors, and narrow rendering.

mod common;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ferrosonic::app::page_state::LyricsStatus;
use ferrosonic::app::App;
use ferrosonic::config::Config;
use ferrosonic::ipc::{DaemonClient, DaemonEvent, DaemonRequest, DaemonResponse, IpcError};
use ferrosonic::subsonic::models::{LyricLine, LyricsSource};
use tokio::sync::broadcast;

struct LyricsClient {
    calls: AtomicUsize,
    delay: Duration,
    fail: bool,
    events: broadcast::Sender<DaemonEvent>,
}

impl LyricsClient {
    fn new(delay: Duration, fail: bool) -> Arc<Self> {
        let (events, _) = broadcast::channel(4);
        Arc::new(Self {
            calls: AtomicUsize::new(0),
            delay,
            fail,
            events,
        })
    }
}

#[async_trait]
impl DaemonClient for LyricsClient {
    async fn request(&self, request: DaemonRequest) -> Result<DaemonResponse, IpcError> {
        if matches!(request, DaemonRequest::FetchLyrics { .. }) {
            self.calls.fetch_add(1, Ordering::SeqCst);
            tokio::time::sleep(self.delay).await;
            if self.fail {
                return Err(IpcError::Daemon("lyrics unavailable".into()));
            }
            return Ok(DaemonResponse::Lyrics(vec![LyricsSource {
                display_artist: Some("Artist".into()),
                display_title: Some("Title".into()),
                lang: Some("en".into()),
                offset: 0,
                synced: true,
                lines: vec![LyricLine {
                    start: Some(0),
                    value: "First line".into(),
                }],
            }]));
        }
        Ok(DaemonResponse::Ok)
    }

    fn subscribe(&self) -> broadcast::Receiver<DaemonEvent> {
        self.events.subscribe()
    }
}

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

async fn app_with_song(client: Arc<LyricsClient>) -> App {
    let app = App::with_remote_client(client, Config::new());
    app.daemon_state.write().await.now_playing.song = Some(common::song("song-1", "Title"));
    app
}

#[tokio::test]
async fn slow_lyrics_request_does_not_block_input_and_result_is_cached() {
    let client = LyricsClient::new(Duration::from_millis(150), false);
    let mut app = app_with_song(client.clone()).await;

    tokio::time::timeout(
        Duration::from_millis(50),
        app.handle_key(key(KeyCode::Char('y'))),
    )
    .await
    .expect("opening lyrics must not wait for the server")
    .unwrap();
    assert!(matches!(
        app.client_state.read().await.lyrics.status,
        LyricsStatus::Loading
    ));

    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(matches!(
        app.client_state.read().await.lyrics.status,
        LyricsStatus::Ready(_)
    ));

    app.handle_key(key(KeyCode::Char('y'))).await.unwrap();
    app.handle_key(key(KeyCode::Char('y'))).await.unwrap();
    assert_eq!(client.calls.load(Ordering::SeqCst), 1);

    app.daemon_state.write().await.now_playing.song = Some(common::song("song-2", "Next Title"));
    app.tick_post().await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(client.calls.load(Ordering::SeqCst), 2);
    assert_eq!(
        app.client_state.read().await.lyrics.song_id.as_deref(),
        Some("song-2")
    );
}

#[tokio::test]
async fn failed_lyrics_request_becomes_overlay_error() {
    let client = LyricsClient::new(Duration::ZERO, true);
    let mut app = app_with_song(client).await;
    app.handle_key(key(KeyCode::Char('y'))).await.unwrap();
    tokio::task::yield_now().await;
    tokio::time::sleep(Duration::from_millis(5)).await;
    assert!(matches!(
        app.client_state.read().await.lyrics.status,
        LyricsStatus::Error(_)
    ));
}

#[test]
fn narrow_terminal_keeps_lyrics_controls_and_current_line_visible() {
    let mut daemon = ferrosonic::daemon::DaemonState::new(Config::new());
    daemon.now_playing.song = Some(common::song("song-1", "A fairly long song title"));
    daemon.now_playing.position = 2.0;
    let mut client = ferrosonic::app::client_state::ClientState::default();
    client.lyrics.open = true;
    client.lyrics.follow = true;
    client.lyrics.status = LyricsStatus::Ready(vec![LyricsSource {
        display_artist: None,
        display_title: None,
        lang: Some("xxx".into()),
        offset: 0,
        synced: true,
        lines: vec![
            LyricLine {
                start: Some(0),
                value: "First line".into(),
            },
            LyricLine {
                start: Some(1_000),
                value: "Current line".into(),
            },
            LyricLine {
                start: Some(3_000),
                value: "Last line".into(),
            },
        ],
    }]);
    let frame = common::render(50, 18, &daemon, &mut client);
    assert!(frame.contains("Current line"), "{frame}");
    assert!(frame.contains("Follow on"), "{frame}");
    assert!(frame.contains("unspecified"), "{frame}");
    insta::assert_snapshot!("lyrics_overlay_narrow", frame);
}

#[test]
fn follow_scrolls_untimed_lyrics_using_track_progress() {
    let mut daemon = ferrosonic::daemon::DaemonState::new(Config::new());
    daemon.now_playing.song = Some(common::song("song-1", "Untimed"));
    daemon.now_playing.duration = 60.0;
    daemon.now_playing.position = 10.0;
    let mut client = ferrosonic::app::client_state::ClientState::default();
    client.lyrics.open = true;
    client.lyrics.follow = true;
    client.lyrics.status = LyricsStatus::Ready(vec![LyricsSource {
        display_artist: None,
        display_title: None,
        lang: None,
        offset: 0,
        synced: false,
        lines: (0..60)
            .map(|index| LyricLine {
                start: None,
                value: format!("line {index}"),
            })
            .collect(),
    }]);

    let _ = common::render(50, 18, &daemon, &mut client);
    let early_scroll = client.lyrics.scroll;
    daemon.now_playing.position = 50.0;
    let late_frame = common::render(50, 18, &daemon, &mut client);
    assert!(client.lyrics.scroll > early_scroll);
    assert!(late_frame.contains("line 50"), "{late_frame}");
}
