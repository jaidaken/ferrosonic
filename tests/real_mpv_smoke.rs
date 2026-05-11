//! Real-mpv smoke test. Skipped when `mpv` is not on PATH.

use ferrosonic::audio::mpv::MpvController;

fn mpv_available() -> bool {
    std::process::Command::new("mpv")
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

#[tokio::test]
async fn real_mpv_starts_and_round_trips_basic_commands() {
    if !mpv_available() {
        eprintln!("skipping: mpv binary not on PATH");
        return;
    }

    let tempdir = tempfile::tempdir().expect("tempdir for mpv socket");
    let socket = tempdir.path().join("real-mpv.sock");
    let mut mpv = MpvController::with_socket_path(socket);
    mpv.start().await.expect("start real mpv");

    assert!(mpv.is_running(), "mpv should be running after start");

    mpv.pause()
        .await
        .expect("real mpv accepts set_property pause=true");
    let paused = mpv
        .is_paused()
        .await
        .expect("real mpv answers get_property pause");
    assert!(paused, "real mpv reports pause=true after our pause()");

    mpv.resume().await.expect("real mpv accepts unpause");
    let paused = mpv.is_paused().await.expect("real mpv answers pause again");
    assert!(!paused, "real mpv reports pause=false after our resume()");
}
