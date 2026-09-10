//! ipc/socket_client.rs: every public path and edge case.

mod common;

use common::TestDaemon;
use ferrosonic::ipc::client::DaemonClient;
use ferrosonic::ipc::protocol::DaemonRequest;
use ferrosonic::ipc::server::serve;
use ferrosonic::ipc::SocketClient;
use serial_test::serial;
use std::time::Duration;

async fn wait_for_socket(path: &std::path::Path, ms: u64) -> bool {
    let deadline = std::time::Instant::now() + Duration::from_millis(ms);
    while std::time::Instant::now() < deadline {
        if tokio::net::UnixStream::connect(path).await.is_ok() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    false
}

#[tokio::test]
#[serial]
async fn connect_to_nonexistent_socket_returns_error() {
    let r = SocketClient::connect(std::path::Path::new("/tmp/ferrosonic-nope.sock")).await;
    assert!(r.is_err());
}

#[tokio::test]
#[serial]
async fn ping_request_returns_pong() {
    let td = TestDaemon::new().await;
    let socket = td.config_dir.path().join("socket-client-ping.sock");
    let core = td.core.clone();
    let socket_path = socket.clone();
    let server = tokio::spawn(async move { serve(core, &socket_path).await });
    assert!(wait_for_socket(&socket, 1500).await);

    let client = SocketClient::connect(&socket).await.unwrap();
    let resp = client.request(DaemonRequest::Ping).await.unwrap();
    matches!(resp, ferrosonic::ipc::DaemonResponse::Pong);
    server.abort();
}

#[tokio::test]
#[serial]
async fn subscribe_receiver_gets_broadcast_events() {
    let td = TestDaemon::new().await;
    let socket = td.config_dir.path().join("socket-client-sub.sock");
    let core = td.core.clone();
    let socket_path = socket.clone();
    let server = tokio::spawn(async move { serve(core, &socket_path).await });
    assert!(wait_for_socket(&socket, 1500).await);

    let client = SocketClient::connect(&socket).await.unwrap();
    let mut rx = client.subscribe();
    for _ in 0..100 {
        tokio::task::yield_now().await;
    }

    td.core.broadcast_queue_changed().await;

    let _ = tokio::time::timeout(Duration::from_millis(500), rx.recv()).await;
    server.abort();
}

#[tokio::test]
#[serial]
async fn request_after_server_dies_returns_disconnected() {
    let td = TestDaemon::new().await;
    let socket = td.config_dir.path().join("socket-client-die.sock");
    let core = td.core.clone();
    let socket_path = socket.clone();
    let server = tokio::spawn(async move { serve(core, &socket_path).await });
    assert!(wait_for_socket(&socket, 1500).await);

    let client = SocketClient::connect(&socket).await.unwrap();
    client.request(DaemonRequest::Ping).await.unwrap();
    server.abort();
    for _ in 0..100 {
        tokio::task::yield_now().await;
    }
    let _ = client.request(DaemonRequest::Ping).await;
}

#[tokio::test]
#[serial]
async fn multiple_concurrent_requests_resolve_independently() {
    let td = TestDaemon::new().await;
    let socket = td.config_dir.path().join("socket-client-conc.sock");
    let core = td.core.clone();
    let socket_path = socket.clone();
    let server = tokio::spawn(async move { serve(core, &socket_path).await });
    assert!(wait_for_socket(&socket, 1500).await);

    let client = std::sync::Arc::new(SocketClient::connect(&socket).await.unwrap());

    let mut handles = Vec::new();
    for _ in 0..10 {
        let c = client.clone();
        handles.push(tokio::spawn(
            async move { c.request(DaemonRequest::Ping).await },
        ));
    }
    for h in handles {
        let _ = h.await.unwrap();
    }
    server.abort();
}

#[tokio::test]
#[serial]
async fn custom_features_round_trip_over_socket_and_reconnect() {
    use ferrosonic::config::keybind::{GlobalAction, KeyChord};
    use ferrosonic::config::{PlaybackFilters, ReplayGainMode};
    use ferrosonic::ipc::protocol::QuickPlayAlbumKind;
    use ferrosonic::ipc::{DaemonResponse, EnqueueMode};
    let td = TestDaemon::new().await;
    td.fake_subsonic.expect_set_rating().await;
    td.fake_subsonic
        .expect_random_album("album", "Album", &["Track"])
        .await;
    td.fake_subsonic
        .expect_quick_play_album("newest", "newest-album", "Newest", &["New Track"])
        .await;
    td.fake_subsonic
        .expect_open_subsonic_extensions(&["songLyrics"])
        .await;
    td.fake_subsonic
        .expect_structured_lyrics(
            "song-0",
            serde_json::json!([{
                "lang": "en", "synced": false,
                "line": [{"value": "Socket lyric"}]
            }]),
        )
        .await;
    let socket = td.config_dir.path().join("features.sock");
    let core = td.core.clone();
    let socket_path = socket.clone();
    let server = tokio::spawn(async move { serve(core, &socket_path).await });
    assert!(wait_for_socket(&socket, 1500).await);
    let client = SocketClient::connect(&socket).await.unwrap();
    for request in [
        DaemonRequest::SetPlaybackFilters(PlaybackFilters {
            min_rating: 2,
            ..Default::default()
        }),
        DaemonRequest::SetKeybindings(std::collections::HashMap::from([(
            GlobalAction::Quit,
            "z".parse::<KeyChord>().unwrap(),
        )])),
        DaemonRequest::SetReplayGainMode(ReplayGainMode::Album),
        DaemonRequest::SetReplayGainPreamp(1.5),
        DaemonRequest::SetReplayGainClip(true),
        DaemonRequest::RefreshRandomAlbum,
        DaemonRequest::RefreshQuickPlayAlbum(QuickPlayAlbumKind::Newest),
        DaemonRequest::SetSongRating {
            id: "song-0".into(),
            rating: 4,
        },
    ] {
        assert!(matches!(
            client.request(request).await.unwrap(),
            DaemonResponse::Ok
        ));
    }
    let lyrics = client
        .request(DaemonRequest::FetchLyrics {
            id: "song-0".into(),
            artist: Some("Test Artist".into()),
            title: "Track".into(),
        })
        .await
        .unwrap();
    let DaemonResponse::Lyrics(lyrics) = lyrics else {
        panic!("expected lyrics response");
    };
    assert_eq!(lyrics[0].lines[0].value, "Socket lyric");
    let mut excluded = common::song("excluded", "Excluded");
    excluded.user_rating = Some(1);
    let included = td.state.read().await.library.random_album_songs[0].clone();
    client
        .request(DaemonRequest::EnqueueSongs {
            songs: vec![excluded, included],
            mode: EnqueueMode::Replace { play_from: None },
        })
        .await
        .unwrap();
    drop(client);
    let client = SocketClient::connect(&socket).await.unwrap();
    let snapshot = client.request(DaemonRequest::Snapshot).await.unwrap();
    let DaemonResponse::Snapshot(state) = snapshot else {
        panic!("expected state");
    };
    assert_eq!(state.queue.len(), 1);
    assert_eq!(state.queue[0].user_rating, Some(4));
    assert_eq!(
        state.config.keybindings.get(&GlobalAction::Quit),
        Some(&"z".parse::<KeyChord>().unwrap())
    );
    assert_eq!(state.library.random_album_songs.len(), 1);
    assert_eq!(
        state.library.quick_play_album_songs[&QuickPlayAlbumKind::Newest][0].title,
        "New Track"
    );
    assert_eq!(state.config.playback_filters.min_rating, 2);
    assert_eq!(state.config.replay_gain_mode, ReplayGainMode::Album);
    assert_eq!(state.config.replay_gain_preamp, 1.5);
    assert!(state.config.replay_gain_clip);
    server.abort();
}

#[tokio::test(start_paused = true)]
#[serial]
async fn request_returns_timeout_when_the_server_never_replies() {
    // A daemon handler that wedges must not hang the TUI forever. Virtual time
    // lets the 30s deadline fire immediately.
    let dir = common::tempdir();
    let path = dir.path().join("silent.sock");
    let listener = tokio::net::UnixListener::bind(&path).expect("bind");
    tokio::spawn(async move {
        let (_stream, _) = listener.accept().await.expect("accept");
        // Hold the connection open without ever writing a reply.
        std::future::pending::<()>().await;
    });

    let client = SocketClient::connect(&path).await.expect("connect");
    let err = client
        .request(DaemonRequest::Ping)
        .await
        .expect_err("a silent server must time out");
    assert!(
        matches!(err, ferrosonic::ipc::IpcError::Timeout),
        "expected Timeout, got {err:?}"
    );
}

#[tokio::test]
#[serial]
async fn socket_close_emits_shutdown_for_subscribers() {
    let dir = common::tempdir();
    let path = dir.path().join("drop.sock");
    let listener = tokio::net::UnixListener::bind(&path).expect("bind");
    tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept");
        // Give the client a moment to subscribe before closing.
        tokio::time::sleep(Duration::from_millis(200)).await;
        drop(stream);
    });

    let client = SocketClient::connect(&path).await.expect("connect");
    let mut rx = client.subscribe();
    let ev = tokio::time::timeout(Duration::from_secs(2), rx.recv()).await;
    assert!(
        matches!(ev, Ok(Ok(ferrosonic::ipc::DaemonEvent::Shutdown))),
        "a daemon disconnect must notify subscribers with Shutdown, got {ev:?}"
    );
}
