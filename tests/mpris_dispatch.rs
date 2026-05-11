//! MPRIS dispatch: PlayerInterface methods route to the right DaemonRequest.

mod common;

use std::sync::Arc;

use common::RecordingClient;
use ferrosonic::app::state::{
    new_shared_client_state, new_shared_daemon_state, PlaybackState, SharedDaemonState,
};
use ferrosonic::config::Config;
use ferrosonic::ipc::protocol::DaemonRequest;
use ferrosonic::mpris::server::MprisPlayer;
use mpris_server::{PlaybackStatus, PlayerInterface, Time};

fn build_player() -> (MprisPlayer, Arc<RecordingClient>, SharedDaemonState) {
    let config = Config::new();
    let daemon_state = new_shared_daemon_state(config.clone());
    let client_state = new_shared_client_state(&config);
    let rec = RecordingClient::new();
    let player = MprisPlayer::new(daemon_state.clone(), client_state, rec.clone());
    (player, rec, daemon_state)
}

async fn drain_fire(rec: &RecordingClient, expected: usize) {
    for _ in 0..50 {
        if rec.requests().await.len() >= expected {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
}

#[tokio::test]
async fn mpris_stop_dispatches_daemon_stop() {
    let (player, rec, _) = build_player();
    player.stop().await.unwrap();
    drain_fire(&rec, 1).await;
    let reqs = rec.requests().await;
    assert!(
        matches!(reqs.as_slice(), [DaemonRequest::Stop]),
        "MPRIS Stop must dispatch DaemonRequest::Stop; got {:?}",
        reqs
    );
}

#[tokio::test]
async fn mpris_play_dispatches_resume() {
    let (player, rec, _) = build_player();
    player.play().await.unwrap();
    drain_fire(&rec, 1).await;
    assert!(matches!(
        rec.requests().await.as_slice(),
        [DaemonRequest::Resume]
    ));
}

#[tokio::test]
async fn mpris_pause_dispatches_pause() {
    let (player, rec, _) = build_player();
    player.pause().await.unwrap();
    drain_fire(&rec, 1).await;
    assert!(matches!(
        rec.requests().await.as_slice(),
        [DaemonRequest::Pause]
    ));
}

#[tokio::test]
async fn mpris_play_pause_dispatches_toggle() {
    let (player, rec, _) = build_player();
    player.play_pause().await.unwrap();
    drain_fire(&rec, 1).await;
    assert!(matches!(
        rec.requests().await.as_slice(),
        [DaemonRequest::TogglePause]
    ));
}

#[tokio::test]
async fn mpris_next_and_previous_dispatch_correctly() {
    let (player, rec, _) = build_player();
    player.next().await.unwrap();
    player.previous().await.unwrap();
    drain_fire(&rec, 2).await;
    let reqs = rec.requests().await;
    assert!(matches!(
        reqs.as_slice(),
        [DaemonRequest::Next, DaemonRequest::Previous]
    ));
}

#[tokio::test]
async fn mpris_seek_dispatches_seek_relative_with_seconds() {
    let (player, rec, _) = build_player();
    player.seek(Time::from_micros(2_500_000)).await.unwrap();
    drain_fire(&rec, 1).await;
    let reqs = rec.requests().await;
    match reqs.as_slice() {
        [DaemonRequest::SeekRelative(s)] => {
            assert!((*s - 2.5).abs() < 1e-6, "expected 2.5s, got {}", s);
        }
        other => panic!("expected SeekRelative(2.5), got {:?}", other),
    }
}

#[tokio::test]
async fn mpris_playback_status_reports_daemon_state() {
    let (player, _rec, daemon_state) = build_player();

    {
        let mut s = daemon_state.write().await;
        s.now_playing.state = PlaybackState::Playing;
    }
    assert_eq!(
        player.playback_status().await.unwrap(),
        PlaybackStatus::Playing
    );

    {
        let mut s = daemon_state.write().await;
        s.now_playing.state = PlaybackState::Paused;
    }
    assert_eq!(
        player.playback_status().await.unwrap(),
        PlaybackStatus::Paused
    );

    {
        let mut s = daemon_state.write().await;
        s.now_playing.state = PlaybackState::Stopped;
    }
    assert_eq!(
        player.playback_status().await.unwrap(),
        PlaybackStatus::Stopped
    );
}
