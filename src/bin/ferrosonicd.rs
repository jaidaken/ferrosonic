//! Ferrosonicd binary entry point.

use std::fs::{self, OpenOptions};
use std::path::PathBuf;
use std::sync::Arc;

use clap::Parser;
use tokio::signal::unix::{signal, SignalKind};
use tracing::{error, info, warn};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

use ferrosonic::app::state::new_shared_daemon_state_with_restored_queue;
use ferrosonic::config::paths::config_dir;
use ferrosonic::config::Config;
use ferrosonic::daemon::DaemonCore;
use ferrosonic::ipc::path::socket_path;
use ferrosonic::ipc::server::serve;

#[derive(Parser, Debug)]
#[command(name = "ferrosonicd")]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Path to config file
    #[arg(short, long, value_name = "FILE")]
    config: Option<PathBuf>,

    /// Enable verbose/debug logging
    #[arg(short, long)]
    verbose: bool,
}

fn init_logging(verbose: bool) -> Option<tracing_appender::non_blocking::WorkerGuard> {
    let log_dir = config_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
    if let Err(e) = fs::create_dir_all(&log_dir) {
        eprintln!("Warning: Could not create log directory: {}", e);
        return None;
    }
    let log_file = log_dir.join("ferrosonicd.log");
    let file = match OpenOptions::new().create(true).append(true).open(&log_file) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("Warning: Could not open log file: {}", e);
            return None;
        }
    };
    let (non_blocking, guard) = tracing_appender::non_blocking(file);
    let filter = if verbose {
        EnvFilter::new("ferrosonic=debug")
    } else {
        EnvFilter::new("ferrosonic=info")
    };
    let registry = tracing_subscriber::registry().with(filter).with(
        fmt::layer()
            .with_writer(non_blocking)
            .with_ansi(false)
            .with_target(false),
    );
    #[cfg(feature = "console")]
    let registry = registry.with(console_subscriber::spawn());
    registry.init();
    if verbose {
        eprintln!("Logging to: {}", log_file.display());
    }
    Some(guard)
}

fn install_panic_hook() {
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        tracing::error!("Panic: {}", info);
        prev(info);
    }));
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let _log_guard = init_logging(args.verbose);
    install_panic_hook();

    info!("ferrosonicd starting...");

    let config = match args.config {
        Some(path) => {
            info!("Loading config from {}", path.display());
            Config::load_from_file(&path)?
        }
        None => {
            info!("Loading default config");
            Config::load_default().unwrap_or_else(|e| {
                info!("No config found ({}), using defaults", e);
                Config::new()
            })
        }
    };

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
        _ = &mut Box::pin(server_task) => warn!("Server task ended unexpectedly"),
    }

    shutdown(&core, &path).await;
    info!("ferrosonicd exiting");
    Ok(())
}

async fn shutdown(core: &Arc<DaemonCore>, socket: &std::path::Path) {
    use ferrosonic::ipc::DaemonEvent;
    let _ = core.event_tx.send(DaemonEvent::Shutdown);
    core.quit_mpv().await;
    if let Err(e) = std::fs::remove_file(socket) {
        if e.kind() != std::io::ErrorKind::NotFound {
            warn!("Failed to remove socket {}: {}", socket.display(), e);
        }
    }
}
