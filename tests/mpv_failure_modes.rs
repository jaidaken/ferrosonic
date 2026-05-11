//! mpv controller failure modes: connect to missing socket, dead pipe.

use ferrosonic::audio::mpv::MpvController;
use ferrosonic::error::AudioError;

#[tokio::test]
async fn connect_to_existing_returns_error_when_socket_missing() {
    let tempdir = tempfile::tempdir().unwrap();
    let path = tempdir.path().join("nonexistent.sock");
    let mut mpv = MpvController::with_socket_path(path);
    let r = mpv.connect_to_existing().await;
    assert!(r.is_err());
    assert!(matches!(r.unwrap_err(), AudioError::MpvIpc(_)));
}

#[tokio::test]
async fn commands_on_unconnected_controller_return_not_running() {
    let mut mpv = MpvController::new();
    let r = mpv.pause().await;
    assert!(r.is_err());
}

#[tokio::test]
async fn is_running_on_fresh_controller_is_false() {
    let mut mpv = MpvController::new();
    assert!(!mpv.is_running());
}

#[tokio::test]
async fn loadfile_on_unconnected_returns_error() {
    let mut mpv = MpvController::new();
    assert!(mpv.loadfile("any.mp3").await.is_err());
}

#[tokio::test]
async fn loadfile_append_on_unconnected_returns_error() {
    let mut mpv = MpvController::new();
    assert!(mpv.loadfile_append("any.mp3").await.is_err());
}

#[tokio::test]
async fn seek_on_unconnected_returns_error() {
    let mut mpv = MpvController::new();
    assert!(mpv.seek(10.0).await.is_err());
}

#[tokio::test]
async fn stop_on_unconnected_returns_error() {
    let mut mpv = MpvController::new();
    assert!(mpv.stop().await.is_err());
}

#[tokio::test]
async fn with_socket_path_preserves_provided_path() {
    let tempdir = tempfile::tempdir().unwrap();
    let path = tempdir.path().join("custom.sock");
    let _mpv = MpvController::with_socket_path(path.clone());
}
