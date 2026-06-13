//! Signal handling + TerminalGuard for clean shutdown.

use crate::app::state::SharedClientState;

/// Pure function: sets the quit flag. Tests call this directly.
pub async fn handle_signal_received(client_state: SharedClientState) {
    let mut s = client_state.write().await;
    s.should_quit = true;
}

/// Spawn a task that resolves `signal_fut` then sets should_quit; tests pass any Future, production passes `wait_for_unix_quit_signal()`.
pub fn spawn_quit_listener<F>(client_state: SharedClientState, signal_fut: F)
where
    F: std::future::Future<Output = ()> + Send + 'static,
{
    tokio::spawn(async move {
        signal_fut.await;
        handle_signal_received(client_state).await;
    });
}

/// Resolves when any of SIGTERM / SIGINT / SIGHUP fires, or returns pending forever if signal registration fails.
pub async fn wait_for_unix_quit_signal() {
    use tokio::signal::unix::{signal, SignalKind};
    let mut term = match signal(SignalKind::terminate()) {
        Ok(s) => s,
        Err(_) => {
            std::future::pending::<()>().await;
            return;
        }
    };
    let mut int = match signal(SignalKind::interrupt()) {
        Ok(s) => s,
        Err(_) => {
            std::future::pending::<()>().await;
            return;
        }
    };
    let mut hup = match signal(SignalKind::hangup()) {
        Ok(s) => s,
        Err(_) => {
            std::future::pending::<()>().await;
            return;
        }
    };
    tokio::select! {
        _ = term.recv() => {}
        _ = int.recv() => {}
        _ = hup.recv() => {}
    }
}

/// RAII guard restoring the terminal (raw mode, alt screen, mouse) on drop.
pub struct TerminalGuard {
    cleanup: Option<Box<dyn FnOnce() + Send>>,
}

impl TerminalGuard {
    /// Guard that undoes crossterm raw mode, alternate screen, and mouse capture.
    pub fn new_crossterm() -> Self {
        Self {
            cleanup: Some(Box::new(|| {
                let _ = crossterm::terminal::disable_raw_mode();
                let _ = crossterm::execute!(
                    std::io::stdout(),
                    crossterm::terminal::LeaveAlternateScreen,
                    crossterm::event::DisableMouseCapture
                );
            })),
        }
    }

    /// Test seam: cleanup closure runs when this guard is dropped.
    pub fn with_cleanup<F>(cleanup: F) -> Self
    where
        F: FnOnce() + Send + 'static,
    {
        Self {
            cleanup: Some(Box::new(cleanup)),
        }
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        if let Some(c) = self.cleanup.take() {
            c();
        }
    }
}
