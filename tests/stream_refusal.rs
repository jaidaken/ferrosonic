//! A stream the server refuses is reported to the user, never skipped
//! silently. Covers the buffered download path (`prebuffer_and_load`) and
//! the mpv load path (`end-file` with reason `error`, which covers a direct
//! load and a gapless preload).

mod common;

use std::time::Duration;

use common::{song, songs, TestDaemon};
use ferrosonic::daemon::core::PlayMode;
use ferrosonic::daemon::state::PlaybackState;
use ferrosonic::ipc::protocol::DaemonEvent;
use ferrosonic::subsonic::models::Child;
use serde_json::{json, Value};
use serial_test::serial;
use tokio::sync::broadcast::Receiver;
use wiremock::ResponseTemplate;

/// Messages that start with "Cannot play", collected for `window_ms`.
async fn cannot_play_messages(rx: &mut Receiver<DaemonEvent>, window_ms: u64) -> Vec<String> {
    let deadline = tokio::time::Instant::now() + Duration::from_millis(window_ms);
    let mut out = Vec::new();
    while let Ok(Ok(ev)) = tokio::time::timeout_at(deadline, rx.recv()).await {
        if let DaemonEvent::Notification { message, is_error } = ev {
            if message.starts_with("Cannot play") {
                assert!(is_error, "a refused stream is an error: {message}");
                out.push(message);
            }
        }
    }
    out
}

/// The first "Cannot play" message within `window_ms`, if any.
async fn first_cannot_play(rx: &mut Receiver<DaemonEvent>, window_ms: u64) -> Option<String> {
    let deadline = tokio::time::Instant::now() + Duration::from_millis(window_ms);
    while let Ok(Ok(ev)) = tokio::time::timeout_at(deadline, rx.recv()).await {
        if let DaemonEvent::Notification { message, .. } = ev {
            if message.starts_with("Cannot play") {
                return Some(message);
            }
        }
    }
    None
}

fn loaded_prebuffer_file(cmds: &[Vec<Value>]) -> bool {
    cmds.iter().any(|c| {
        c.first().and_then(Value::as_str) == Some("loadfile")
            && c.get(1)
                .and_then(Value::as_str)
                .is_some_and(|p| p.contains("ferrosonic-prebuf-"))
    })
}

async fn playing(td: &TestDaemon, queue: Vec<Child>) {
    let mut s = td.state.write().await;
    s.queue = queue;
    s.queue_position = Some(0);
    s.now_playing.song = Some(s.queue[0].clone());
    s.now_playing.state = PlaybackState::Playing;
    s.now_playing.duration = 180.0;
}

fn subsonic_xml_error(code: u32, message: &str) -> ResponseTemplate {
    // `set_body_raw` sets the type; `set_body_string` forces text/plain.
    ResponseTemplate::new(200).set_body_raw(
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<subsonic-response xmlns="http://subsonic.org/restapi" status="failed" version="1.16.1"><error code="{code}" message="{message}"/></subsonic-response>"#
        ),
        "text/xml; charset=utf-8",
    )
}

#[tokio::test]
#[serial]
async fn buffered_play_of_a_missing_song_names_the_server_reason() {
    let td = TestDaemon::new().await;
    td.fake_subsonic
        .expect_stream_reply("gone", subsonic_xml_error(70, "Song not found"))
        .await;
    td.state
        .write()
        .await
        .queue
        .extend([song("gone", "Track A"), song("next", "Track B")]);
    let mut rx = td.core.subscribe();

    td.core
        .play_queue_position(0, PlayMode::Buffered)
        .await
        .unwrap();

    assert_eq!(
        first_cannot_play(&mut rx, 5000).await.as_deref(),
        Some(
            r#"Cannot play "Track A": the server refused it: Song not found (Subsonic error 70)."#
        )
    );
    assert!(
        !loaded_prebuffer_file(&td.fake_mpv.commands().await),
        "the error document must never reach mpv as a song file"
    );
    assert_eq!(
        td.state.read().await.now_playing.state,
        PlaybackState::Playing,
        "a refusal of one song leaves playback free to move to the next song"
    );
}

#[tokio::test]
#[serial]
async fn buffered_play_with_rejected_credentials_stops_and_keeps_the_queue() {
    let td = TestDaemon::new().await;
    td.fake_subsonic
        .expect_stream_reply(
            "a",
            ResponseTemplate::new(200).set_body_json(json!({
                "subsonic-response": {
                    "status": "failed",
                    "version": "1.16.1",
                    "error": { "code": 40, "message": "Wrong username or password" }
                }
            })),
        )
        .await;
    td.state
        .write()
        .await
        .queue
        .extend([song("a", "Track A"), song("b", "Track B")]);
    let mut rx = td.core.subscribe();

    td.core
        .play_queue_position(0, PlayMode::Buffered)
        .await
        .unwrap();

    assert_eq!(
        first_cannot_play(&mut rx, 5000).await.as_deref(),
        Some(
            r#"Cannot play "Track A": the server refused it: Wrong username or password (Subsonic error 40). Playback stopped."#
        )
    );
    let s = td.state.read().await;
    assert_eq!(s.now_playing.state, PlaybackState::Stopped);
    assert_eq!(s.queue.len(), 2, "the queue stays for a later Play");
    assert_eq!(s.queue_position, Some(0));
}

#[tokio::test]
#[serial]
async fn failed_mpv_load_reports_the_http_status() {
    let td = TestDaemon::new().await;
    playing(&td, songs("t", 3)).await;
    td.fake_subsonic
        .expect_stream_reply("t-0", ResponseTemplate::new(404))
        .await;
    let mut rx = td.core.subscribe();
    let _listener = td.core.spawn_mpv_event_listener().await;

    td.fake_mpv
        .emit_event(
            json!({ "event": "end-file", "reason": "error", "file_error": "loading failed" }),
        )
        .await;

    assert_eq!(
        first_cannot_play(&mut rx, 5000).await.as_deref(),
        Some(r#"Cannot play "Track 0": the server refused it: HTTP 404 Not Found."#)
    );
    assert_eq!(
        td.state.read().await.now_playing.state,
        PlaybackState::Playing
    );
}

#[tokio::test]
#[serial]
async fn failed_mpv_load_with_audio_from_the_server_sends_no_refusal() {
    let td = TestDaemon::new().await;
    playing(&td, songs("t", 3)).await;
    td.fake_subsonic
        .expect_stream_for("t-0", vec![0u8; 4096])
        .await;
    let mut rx = td.core.subscribe();
    let _listener = td.core.spawn_mpv_event_listener().await;

    td.fake_mpv
        .emit_event(json!({ "event": "end-file", "reason": "error", "file_error": "unrecognized file format" }))
        .await;

    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while td.fake_subsonic.stream_requests("t-0").await == 0 {
        assert!(
            tokio::time::Instant::now() < deadline,
            "the daemon must ask the server about the failed song"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(
        cannot_play_messages(&mut rx, 500).await,
        Vec::<String>::new()
    );
}

#[tokio::test]
#[serial]
async fn unreachable_server_stops_playback_after_a_failed_load() {
    let td = TestDaemon::new().await;
    let closed = {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        l.local_addr().unwrap().port()
    };
    let _ = td
        .core
        .update_server_config(
            &format!("http://127.0.0.1:{closed}/"),
            "test",
            &ferrosonic::secret::Secret::from_string("test".into()),
        )
        .await;
    playing(&td, songs("t", 3)).await;
    let mut rx = td.core.subscribe();
    let _listener = td.core.spawn_mpv_event_listener().await;

    td.fake_mpv
        .emit_event(
            json!({ "event": "end-file", "reason": "error", "file_error": "loading failed" }),
        )
        .await;

    let msg = first_cannot_play(&mut rx, 5000).await.unwrap_or_default();
    assert!(
        msg.starts_with(r#"Cannot play "Track 0": the server did not answer: "#),
        "got {msg:?}"
    );
    assert!(msg.ends_with(". Playback stopped."), "got {msg:?}");
    assert!(
        !msg.contains("t="),
        "the message must not carry the auth token: {msg:?}"
    );
    assert_eq!(
        td.state.read().await.now_playing.state,
        PlaybackState::Stopped
    );
}

/// Error document for song `t-0`, behind `padding` bytes of XML comment.
async fn padded_error_reply(td: &TestDaemon, padding: usize) {
    let body = format!(
        r#"<?xml version="1.0"?><!--{}--><subsonic-response status="failed"><error code="70" message="Song not found"/></subsonic-response>"#,
        " ".repeat(padding)
    );
    td.fake_subsonic
        .expect_stream_reply(
            "t-0",
            ResponseTemplate::new(200).set_body_raw(body, "text/xml"),
        )
        .await;
}

#[tokio::test]
#[serial]
async fn error_document_with_4_kib_of_padding_still_gives_its_reason() {
    let td = TestDaemon::new().await;
    playing(&td, songs("t", 2)).await;
    padded_error_reply(&td, 4 * 1024).await;
    let mut rx = td.core.subscribe();
    let _listener = td.core.spawn_mpv_event_listener().await;

    td.fake_mpv
        .emit_event(json!({ "event": "end-file", "reason": "error" }))
        .await;

    assert_eq!(
        first_cannot_play(&mut rx, 5000).await.as_deref(),
        Some(
            r#"Cannot play "Track 0": the server refused it: Song not found (Subsonic error 70)."#
        )
    );
}

#[tokio::test]
#[serial]
async fn error_reply_past_64_kib_is_not_read_in_full() {
    let td = TestDaemon::new().await;
    playing(&td, songs("t", 2)).await;
    padded_error_reply(&td, 80 * 1024).await;
    let mut rx = td.core.subscribe();
    let _listener = td.core.spawn_mpv_event_listener().await;

    td.fake_mpv
        .emit_event(json!({ "event": "end-file", "reason": "error" }))
        .await;

    assert_eq!(
        first_cannot_play(&mut rx, 5000).await.as_deref(),
        Some(r#"Cannot play "Track 0": the server sent text/xml instead of audio."#),
        "only the first 64 KiB are read, so the error element past it stays unseen"
    );
}

#[tokio::test]
async fn client_transport_error_text_hides_the_auth_parameters() {
    let closed = {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        l.local_addr().unwrap().port()
    };
    let client = ferrosonic::subsonic::SubsonicClient::new(
        &format!("http://127.0.0.1:{closed}/"),
        "alice",
        &ferrosonic::secret::Secret::from_string("pw".into()),
    )
    .unwrap();

    let text = client.ping().await.unwrap_err().to_string();

    assert!(text.starts_with("HTTP request failed: "), "got {text:?}");
    for param in ["t=", "s=", "u=alice"] {
        assert!(!text.contains(param), "{param} leaked into {text:?}");
    }
}

#[tokio::test]
#[serial]
async fn refused_radio_station_is_reported_without_a_stop() {
    let td = TestDaemon::new().await;
    let url = td
        .fake_subsonic
        .expect_raw_reply("/station", ResponseTemplate::new(403))
        .await;
    let mut station = song("radio:1", "Night FM");
    station.radio_stream_url = Some(url);
    playing(&td, vec![station, song("s", "Track S")]).await;
    let mut rx = td.core.subscribe();
    let _listener = td.core.spawn_mpv_event_listener().await;

    td.fake_mpv
        .emit_event(
            json!({ "event": "end-file", "reason": "error", "file_error": "loading failed" }),
        )
        .await;

    assert_eq!(
        first_cannot_play(&mut rx, 5000).await.as_deref(),
        Some(r#"Cannot play "Night FM": the server refused it: HTTP 403 Forbidden."#)
    );
    assert_eq!(
        td.state.read().await.now_playing.state,
        PlaybackState::Playing,
        "a station's 403 says nothing about the Subsonic account"
    );
}
