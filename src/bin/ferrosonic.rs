use std::fs::{self, OpenOptions};
use std::path::PathBuf;
use std::time::Duration;

use clap::Parser;
use tracing::info;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

use ferrosonic::app::spawn_daemon::spawn_and_wait;
use ferrosonic::app::App;
use ferrosonic::config::paths::config_dir;
use ferrosonic::config::Config;
use ferrosonic::ipc::path::socket_path;
use ferrosonic::ipc::SocketClient;

const DAEMON_SPAWN_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Parser, Debug)]
#[command(name = "ferrosonic")]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(short, long, value_name = "FILE")]
    config: Option<PathBuf>,

    #[arg(short, long)]
    verbose: bool,

    /// Skip the daemon auto-spawn / connect; run in-process.
    #[arg(long)]
    standalone: bool,

    /// Internal: run as the background daemon. The TUI re-execs itself with
    /// this; not for direct use.
    #[arg(long, hide = true)]
    daemon: bool,
}

/// Returned guard must outlive the program; dropping it ends the
/// non-blocking writer task. The daemon mode logs to a separate file.
fn init_logging(verbose: bool, daemon: bool) -> Option<tracing_appender::non_blocking::WorkerGuard> {
    let log_dir = config_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
    if let Err(e) = fs::create_dir_all(&log_dir) {
        eprintln!("Warning: Could not create log directory: {}", e);
        return None;
    }
    let log_name = if daemon {
        "ferrosonicd.log"
    } else {
        "ferrosonic.log"
    };
    let log_file = log_dir.join(log_name);
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

/// Restore the terminal on panic so the user isn't left in raw mode
/// after a crash.
fn install_tui_panic_hook() {
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = crossterm::terminal::disable_raw_mode();
        let _ = crossterm::execute!(
            std::io::stdout(),
            crossterm::terminal::LeaveAlternateScreen,
            crossterm::event::DisableMouseCapture,
            crossterm::cursor::Show,
        );
        tracing::error!("Panic: {}", info);
        prev(info);
    }));
}

fn install_daemon_panic_hook() {
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        tracing::error!("Panic: {}", info);
        prev(info);
    }));
}

fn load_config(path: Option<&std::path::Path>) -> anyhow::Result<Config> {
    match path {
        Some(path) => {
            info!("Loading config from {}", path.display());
            Ok(Config::load_from_file(path)?)
        }
        None => {
            info!("Loading default config");
            Ok(Config::load_default().unwrap_or_else(|e| {
                info!("No config found ({}), using defaults", e);
                Config::new()
            }))
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let _log_guard = init_logging(args.verbose, args.daemon);

    let config = load_config(args.config.as_deref())?;

    // Internal daemon mode: the TUI re-execs the binary with --daemon.
    if args.daemon {
        install_daemon_panic_hook();
        return ferrosonic::daemon::run(config).await;
    }

    install_tui_panic_hook();
    info!("Ferrosonic starting...");

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
                info!("Connected to the daemon at {}", path.display());
                App::with_remote_client(client, config)
            }
            None => {
                let daemon_log = config_dir()
                    .unwrap_or_else(|| PathBuf::from("/tmp"))
                    .join("ferrosonicd.log");
                eprintln!("ferrosonic: could not reach the background daemon.");
                eprintln!();
                eprintln!("  Socket path : {}", path.display());
                eprintln!("  Daemon log  : {}", daemon_log.display());
                eprintln!();
                eprintln!("Try one of:");
                eprintln!("  - Inspect the daemon log for spawn errors.");
                eprintln!("  - Remove a stale socket: rm {}", path.display());
                eprintln!("  - Run with --standalone to skip the daemon this session.");
                eprintln!("  - Set Daemon=false in your config to disable persistent playback.");
                anyhow::bail!("daemon unreachable; see message above");
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
