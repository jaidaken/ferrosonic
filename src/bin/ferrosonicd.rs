//! Ferrosonicd — long-lived daemon that owns mpv, the queue, the
//! library cache, and the MPRIS server. Accepts TUI clients over a
//! Unix socket (path comes from `ferrosonic::ipc::path::socket_path`).
//!
//! Lifecycle: started either explicitly (`ferrosonicd`) or auto-spawned
//! by the TUI client when the socket is missing (phase 7). Foreground
//! by default; daemonisation under `--detach` is phase 7's job.

use std::fs::{self, OpenOptions};
use std::path::PathBuf;
use std::sync::Arc;

use clap::Parser;
use tracing::{error, info, warn};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

use ferrosonic::app::state::new_shared_state;
use ferrosonic::config::paths::config_dir;
use ferrosonic::config::Config;
use ferrosonic::daemon::DaemonCore;
use ferrosonic::ipc::path::socket_path;
use ferrosonic::ipc::server::serve;

/// Ferrosonicd - Background daemon for ferrosonic
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
    let file = match OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_file)
    {
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
    tracing_subscriber::registry()
        .with(filter)
        .with(
            fmt::layer()
                .with_writer(non_blocking)
                .with_ansi(false)
                .with_target(false),
        )
        .init();
    if verbose {
        eprintln!("Logging to: {}", log_file.display());
    }
    Some(guard)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let _log_guard = init_logging(args.verbose);

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

    // Phase 5a: DaemonCore wraps the full SharedState. Phase 5b
    // changes this to wrap just `Arc<RwLock<DaemonState>>` once
    // App stops sharing the same lock.
    let state = new_shared_state(config.clone());
    let core = DaemonCore::new(state, &config);

    // Start mpv (audio backend) up-front; clients connecting later
    // expect a ready playback session.
    if let Err(e) = core.start_mpv().await {
        warn!("Failed to start mpv: {} - audio playback won't work", e);
    } else {
        info!("mpv started");
    }

    // Spawn the playback poll task. Detached: the runtime takes care
    // of cancellation at process exit.
    let _poll = core.spawn_polling_task();

    // Background-load initial library data so first-connect clients
    // see populated lists.
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
    if let Err(e) = serve(core, &path).await {
        error!("Server terminated: {}", e);
        return Err(e.into());
    }

    info!("ferrosonicd exiting");
    Ok(())
}
