//! MPRIS PlayerInterface + RootInterface getters/setters not yet covered.

mod common;

use std::sync::Arc;

use common::RecordingClient;
use ferrosonic::app::state::{new_shared_client_state, new_shared_daemon_state, SharedDaemonState};
use ferrosonic::config::Config;
use ferrosonic::mpris::server::MprisPlayer;
use mpris_server::{LoopStatus, PlaybackRate, PlayerInterface, RootInterface};

fn build_player() -> (MprisPlayer, Arc<RecordingClient>, SharedDaemonState) {
    let config = Config::new();
    let daemon_state = new_shared_daemon_state(config.clone());
    let client_state = new_shared_client_state(&config);
    let rec = RecordingClient::new();
    let player = MprisPlayer::new(daemon_state.clone(), client_state, rec.clone());
    (player, rec, daemon_state)
}

#[tokio::test]
async fn loop_status_returns_none() {
    let (player, _, _) = build_player();
    assert_eq!(player.loop_status().await.unwrap(), LoopStatus::None);
}

#[tokio::test]
async fn set_loop_status_is_silent_noop() {
    let (player, _, _) = build_player();
    player.set_loop_status(LoopStatus::Track).await.unwrap();
    assert_eq!(player.loop_status().await.unwrap(), LoopStatus::None);
}

#[tokio::test]
async fn shuffle_status_returns_false() {
    let (player, _, _) = build_player();
    assert!(!player.shuffle().await.unwrap());
}

#[tokio::test]
async fn set_shuffle_is_silent_noop() {
    let (player, _, _) = build_player();
    player.set_shuffle(true).await.unwrap();
    assert!(!player.shuffle().await.unwrap());
}

#[tokio::test]
async fn rate_returns_one() {
    let (player, _, _) = build_player();
    let rate: PlaybackRate = player.rate().await.unwrap();
    assert!((rate - 1.0).abs() < 1e-9);
}

#[tokio::test]
async fn set_rate_is_silent_noop() {
    let (player, _, _) = build_player();
    player.set_rate(2.0).await.unwrap();
    assert!((player.rate().await.unwrap() - 1.0).abs() < 1e-9);
}

#[tokio::test]
async fn minimum_and_maximum_rate_match() {
    let (player, _, _) = build_player();
    assert!((player.minimum_rate().await.unwrap() - 1.0).abs() < 1e-9);
    assert!((player.maximum_rate().await.unwrap() - 1.0).abs() < 1e-9);
}

#[tokio::test]
async fn volume_returns_one() {
    let (player, _, _) = build_player();
    let v = player.volume().await.unwrap();
    assert!((v - 1.0).abs() < 1e-9);
}

#[tokio::test]
async fn can_pause_always_true() {
    let (player, _, _) = build_player();
    assert!(player.can_pause().await.unwrap());
}

#[tokio::test]
async fn can_seek_always_true() {
    let (player, _, _) = build_player();
    assert!(player.can_seek().await.unwrap());
}

#[tokio::test]
async fn can_control_always_true() {
    let (player, _, _) = build_player();
    assert!(player.can_control().await.unwrap());
}

#[tokio::test]
async fn open_uri_returns_ok_silently() {
    let (player, _, _) = build_player();
    let result = player.open_uri("http://nope.example".into()).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn root_identity_is_ferrosonic() {
    let (player, _, _) = build_player();
    let id = player.identity().await.unwrap();
    assert!(id.to_lowercase().contains("ferrosonic"));
}

#[tokio::test]
async fn root_desktop_entry_is_ferrosonic() {
    let (player, _, _) = build_player();
    assert_eq!(player.desktop_entry().await.unwrap(), "ferrosonic");
}

#[tokio::test]
async fn root_can_quit_returns_true() {
    let (player, _, _) = build_player();
    assert!(player.can_quit().await.unwrap());
}

#[tokio::test]
async fn root_quit_sets_should_quit_flag() {
    let config = Config::new();
    let daemon_state = new_shared_daemon_state(config.clone());
    let client_state = new_shared_client_state(&config);
    let rec = RecordingClient::new();
    let player = MprisPlayer::new(daemon_state, client_state.clone(), rec);
    player.quit().await.unwrap();
    assert!(client_state.read().await.should_quit);
}

#[tokio::test]
async fn root_raise_is_silent_noop() {
    let (player, _, _) = build_player();
    let result = player.raise().await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn root_fullscreen_is_false() {
    let (player, _, _) = build_player();
    assert!(!player.fullscreen().await.unwrap());
}

#[tokio::test]
async fn root_can_set_fullscreen_is_false() {
    let (player, _, _) = build_player();
    assert!(!player.can_set_fullscreen().await.unwrap());
}

#[tokio::test]
async fn root_set_fullscreen_is_silent_noop() {
    let (player, _, _) = build_player();
    let result = player.set_fullscreen(true).await;
    assert!(result.is_ok());
    assert!(!player.fullscreen().await.unwrap());
}

#[tokio::test]
async fn root_can_raise_is_false() {
    let (player, _, _) = build_player();
    assert!(!player.can_raise().await.unwrap());
}

#[tokio::test]
async fn root_has_track_list_is_false() {
    let (player, _, _) = build_player();
    assert!(!player.has_track_list().await.unwrap());
}

#[tokio::test]
async fn root_supported_uri_schemes_lists_http() {
    let (player, _, _) = build_player();
    let schemes = player.supported_uri_schemes().await.unwrap();
    assert!(schemes.iter().any(|s| s == "http" || s == "https"));
}

#[tokio::test]
async fn root_supported_mime_types_lists_audio() {
    let (player, _, _) = build_player();
    let mimes = player.supported_mime_types().await.unwrap();
    assert!(mimes.iter().any(|m| m.starts_with("audio/")));
}
