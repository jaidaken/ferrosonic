use std::fs::{self, OpenOptions};
use std::path::{Path, PathBuf};
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
fn init_logging(
    verbose: bool,
    daemon: bool,
) -> Option<tracing_appender::non_blocking::WorkerGuard> {
    let log_dir = config_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
    if let Err(e) = fs::create_dir_all(&log_dir) {
        eprintln!("Warning: Could not create log directory: {}", e);
        return None;
    }
    // The config dir holds credentials and logs; keep it owner-only.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&log_dir, fs::Permissions::from_mode(0o700));
    }
    let log_name = if daemon {
        "ferrosonicd.log"
    } else {
        "ferrosonic.log"
    };
    let log_file = log_dir.join(log_name);
    // The log can contain server URLs and, historically, error text; keep it
    // owner-only whether it is newly created or already exists.
    let file = match open_private_log(&log_file) {
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

fn open_private_log(path: &Path) -> std::io::Result<std::fs::File> {
    let mut opts = OpenOptions::new();
    opts.create(true).append(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let file = opts.open(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(fs::Permissions::from_mode(0o600))?;
    }
    Ok(file)
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
    if let Some(path) = path {
        info!("Loading config from {}", path.display());
        Ok(Config::load_from_file(path)?)
    } else {
        info!("Loading default config");
        Ok(Config::load_default().unwrap_or_else(|e| {
            info!("No config found ({}), using defaults", e);
            Config::new()
        }))
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
        if let Some(client) = connect_or_spawn(&path).await {
            info!("Connected to the daemon at {}", path.display());
            App::with_remote_client(client, config)
        } else {
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
    };

    let run_result = app.run().await;
    if let Err(e) = &run_result {
        tracing::error!("Application error: {}", e);
    }
    info!("Ferrosonic exiting...");

    // Force-exit: a stuck blocking task in the dropping runtime can otherwise
    // hang the process after quit (terminal already restored by run()).
    drop(_log_guard);
    std::process::exit(i32::from(run_result.is_err()));
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

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn existing_permissive_log_is_corrected_to_owner_only() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ferrosonic.log");
        fs::write(&path, b"old log\n").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o664)).unwrap();

        let _file = open_private_log(&path).unwrap();
        let mode = fs::metadata(path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }
}
