//! Real-mpv smoke test. Skipped when `mpv` is not on PATH.

mod common;
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

    let tempdir = common::tempdir();
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

/// True while an mpv process whose `--input-ipc-server` arg contains `socket`
/// is alive. Lets the Drop/teardown tests observe real process lifecycle.
fn mpv_proc_alive(socket: &str) -> bool {
    std::process::Command::new("pgrep")
        .arg("-f")
        .arg(socket)
        .output()
        .map(|o| !o.stdout.is_empty())
        .unwrap_or(false)
}

#[tokio::test]
async fn daemon_start_mpv_spawns_and_connects_real_mpv() {
    if !mpv_available() {
        eprintln!("skipping: mpv binary not on PATH");
        return;
    }
    let tempdir = common::tempdir();
    let socket = tempdir.path().join("daemon-start.sock");
    let cfg = ferrosonic::config::Config::new();
    let state = ferrosonic::app::state::new_shared_daemon_state(cfg.clone());
    let core = ferrosonic::daemon::DaemonCore::new_with_mpv(
        state,
        &cfg,
        MpvController::with_socket_path(socket),
    );

    core.start_mpv().await.expect("DaemonCore::start_mpv");

    assert!(
        core.mpv.lock().await.is_running(),
        "start_mpv must actually spawn and connect real mpv"
    );
}

#[tokio::test]
async fn dropping_controller_kills_the_mpv_process_and_removes_the_socket() {
    if !mpv_available() {
        eprintln!("skipping: mpv binary not on PATH");
        return;
    }
    let tempdir = common::tempdir();
    let socket = tempdir.path().join("drop-kill.sock");
    let socket_str = socket.to_string_lossy().to_string();
    {
        let mut mpv = MpvController::with_socket_path(socket.clone());
        mpv.start().await.expect("start real mpv");
        assert!(mpv.is_running(), "mpv running before drop");
        assert!(
            mpv_proc_alive(&socket_str),
            "the real mpv process should be alive before drop"
        );
    } // Drop -> shutdown_sync: kill + wait (sync), abort reader, remove socket.

    assert!(
        !mpv_proc_alive(&socket_str),
        "Drop must kill the mpv process (no leaked child)"
    );
    assert!(!socket.exists(), "Drop must remove the IPC socket file");
}

#[tokio::test]
async fn controller_respawns_mpv_after_an_external_kill() {
    if !mpv_available() {
        eprintln!("skipping: mpv binary not on PATH");
        return;
    }
    let tempdir = common::tempdir();
    let socket = tempdir.path().join("respawn.sock");
    let socket_str = socket.to_string_lossy().to_string();
    let mut mpv = MpvController::with_socket_path(socket);
    mpv.start().await.expect("first start");
    assert!(mpv.is_running(), "running after first start");

    // Simulate a crash: kill the process out from under the controller.
    let _ = std::process::Command::new("pkill")
        .arg("-f")
        .arg(&socket_str)
        .status();

    // The controller observes the dead child and clears state.
    let mut saw_dead = false;
    for _ in 0..200 {
        if !mpv.is_running() {
            saw_dead = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert!(saw_dead, "controller must detect the externally-killed mpv");

    // start() must tear the dead connection down and bring mpv back.
    mpv.start().await.expect("respawn");
    assert!(
        mpv.is_running(),
        "start() must tear down the dead connection and respawn mpv"
    );
}
