//! Ferrosonic - A terminal-based Subsonic music client
//!
//! Features:
//! - Bit-perfect audio playback via MPV and PipeWire sample rate switching
//! - MPRIS2 desktop integration for media controls
//! - Browse artists, albums, and playlists
//! - Play queue with shuffle and reorder support
//! - Server configuration with connection testing

mod app;
mod audio;
mod config;
mod daemon;
mod error;
mod ipc;
mod mpris;
mod subsonic;
mod ui;

use clap::Parser;
use std::fs::{self, OpenOptions};
use std::path::PathBuf;
use tracing::info;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

use crate::app::App;
use crate::config::paths::config_dir;
use crate::config::Config;

/// Ferrosonic - Terminal Subsonic Music Client
#[derive(Parser, Debug)]
#[command(name = "ferrosonic")]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Path to config file
    #[arg(short, long, value_name = "FILE")]
    config: Option<PathBuf>,

    /// Enable verbose/debug logging
    #[arg(short, long)]
    verbose: bool,
}

/// Initialize file-based logging
fn init_logging(verbose: bool) -> Option<tracing_appender::non_blocking::WorkerGuard> {
    // Get log directory (same as config dir for consistency with Go version)
    let log_dir = config_dir().unwrap_or_else(|| PathBuf::from("/tmp"));

    // Create log directory if needed
    if let Err(e) = fs::create_dir_all(&log_dir) {
        eprintln!("Warning: Could not create log directory: {}", e);
        return None;
    }

    let log_file = log_dir.join("ferrosonic.log");

    // Open log file in append mode so crash debugging context survives restarts.
    // (TODO: rotate when file exceeds some size threshold.)
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

    // Initialize file-based logging (keep guard alive for duration of program)
    let _log_guard = init_logging(args.verbose);

    info!("Ferrosonic starting...");

    // Load configuration
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

    // Run the application
    let mut app = App::new(config);
    if let Err(e) = app.run().await {
        tracing::error!("Application error: {}", e);
        return Err(e.into());
    }

    info!("Ferrosonic exiting...");
    Ok(())
}
