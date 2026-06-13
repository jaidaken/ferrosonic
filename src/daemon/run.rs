//! Daemon run loop: owns the `DaemonCore`, serves IPC, exits on signal or
//! a `Shutdown` request. Invoked by the single binary in `--daemon` mode.

use std::sync::Arc;

use tokio::signal::unix::{signal, SignalKind};
use tracing::{error, info, warn};

use crate::app::state::new_shared_daemon_state_with_restored_queue;
use crate::config::Config;
use crate::daemon::DaemonCore;
use crate::ipc::path::socket_path;
use crate::ipc::server::serve;
use crate::ipc::DaemonEvent;

/// Run the daemon until SIGTERM/SIGINT or an IPC `Shutdown` request, then
/// tear down mpv and the socket.
///
/// # Errors
///
/// Returns an error if the SIGTERM/SIGINT signal handlers cannot be registered.
pub async fn run(config: Config) -> anyhow::Result<()> {
    info!("ferrosonicd starting...");
    info!(
        "Server: {}",
        if config.base_url.is_empty() {
            "(not configured)"
        } else {
            &config.base_url
        }
    );

    let daemon_state = new_shared_daemon_state_with_restored_queue(config.clone());
    let core = DaemonCore::new(daemon_state, &config);

    if let Err(e) = core.start_mpv().await {
        warn!("Failed to start mpv: {} - audio playback won't work", e);
    } else {
        info!("mpv started");
    }

    let _poll = core.spawn_polling_task();
    let _mpv_events = core.spawn_mpv_event_listener().await;

    if config.is_configured() {
        let bg = Arc::clone(&core);
        tokio::spawn(async move {
            bg.refresh_starred().await;
            bg.refresh_artists().await;
            bg.refresh_playlists().await;
        });
    }

    let path = socket_path();
    info!("Listening on {}", path.display());

    let serve_core = core.clone();
    let serve_path = path.clone();
    let server_task = tokio::spawn(async move {
        if let Err(e) = serve(serve_core, &serve_path).await {
            error!("Server terminated: {}", e);
        }
    });

    let mut sigterm = signal(SignalKind::terminate())?;
    let mut sigint = signal(SignalKind::interrupt())?;
    tokio::select! {
        _ = sigterm.recv() => info!("SIGTERM received, shutting down"),
        _ = sigint.recv() => info!("SIGINT received, shutting down"),
        res = &mut Box::pin(server_task) => match res {
            Ok(()) => info!("IPC server stopped, shutting down"),
            Err(e) => error!("IPC server task panicked: {}; shutting down", e),
        },
    }

    shutdown(&core, &path).await;
    info!("ferrosonicd exiting");
    Ok(())
}

async fn shutdown(core: &Arc<DaemonCore>, socket: &std::path::Path) {
    let _ = core.event_tx.send(DaemonEvent::Shutdown);
    core.quit_mpv().await;
    if let Err(e) = std::fs::remove_file(socket) {
        if e.kind() != std::io::ErrorKind::NotFound {
            warn!("Failed to remove socket {}: {}", socket.display(), e);
        }
    }
}
