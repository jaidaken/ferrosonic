use std::fs::{self, OpenOptions};
use std::path::PathBuf;
use std::time::Duration;

use clap::Parser;
use tracing::{info, warn};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

use ferrosonic::app::App;
use ferrosonic::config::paths::config_dir;
use ferrosonic::config::Config;
use ferrosonic::ipc::path::socket_path;
use ferrosonic::ipc::spawn::spawn_and_wait;
use ferrosonic::ipc::SocketClient;

const DAEMON_SPAWN_TIMEOUT: Duration = Duration::from_secs(2);

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

    /// Force in-process mode — don't try to connect to ferrosonicd.
    /// Useful for testing or when a daemon connection is undesirable.
    #[arg(long)]
    standalone: bool,
}

/// Initialize file-based logging. Returns the worker guard which must
/// be held for the duration of the program — dropping it shuts down
/// the writer task and any in-flight log lines are lost.
fn init_logging(verbose: bool) -> Option<tracing_appender::non_blocking::WorkerGuard> {
    let log_dir = config_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
    if let Err(e) = fs::create_dir_all(&log_dir) {
        eprintln!("Warning: Could not create log directory: {}", e);
        return None;
    }
    let log_file = log_dir.join("ferrosonic.log");
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

    info!("Ferrosonic starting...");

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

    let mut app = if args.standalone {
        info!("--standalone: forcing in-process mode");
        App::new(config)
    } else if !config.daemon {
        info!("Daemon mode disabled in config; running in-process");
        App::new(config)
    } else {
        let path = socket_path();
        match connect_or_spawn(&path).await {
            Some(client) => {
                info!("Connected to ferrosonicd at {}", path.display());
                App::with_remote_client(client, config)
            }
            None => {
                warn!("ferrosonicd unreachable; running in-process this session");
                App::new(config)
            }
        }
    };

    if let Err(e) = app.run().await {
        tracing::error!("Application error: {}", e);
        return Err(e.into());
    }

    info!("Ferrosonic exiting...");
    Ok(())
}

async fn connect_or_spawn(path: &std::path::Path) -> Option<std::sync::Arc<SocketClient>> {
    if let Ok(client) = SocketClient::connect(path).await {
        return Some(client);
    }
    match spawn_and_wait(path, DAEMON_SPAWN_TIMEOUT).await {
        Ok(()) => SocketClient::connect(path).await.ok(),
        Err(_) => None,
    }
}
