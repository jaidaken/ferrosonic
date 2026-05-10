//! Main application module

mod cava;
pub mod client_state;
mod input;
mod input_artists;
mod input_playlists;
mod input_queue;
mod input_server;
mod input_settings;
mod input_songs;
pub mod models;
mod mouse;
mod mouse_artists;
mod mouse_playlists;
pub mod state;

use std::io;
use std::sync::Arc;
use std::time::Duration;

use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use tracing::{info, warn};

use crate::app::models::SongOption;
use crate::config::Config;
use crate::daemon::DaemonCore;
use crate::error::{Error, UiError};
use crate::ipc::{DaemonClient, DaemonRequest, EnqueueMode, InProcessClient};
use crate::mpris::server::{start_mpris_server, MprisPlayer};
use crate::ui;

pub use state::*;

/// Main application — TUI client side. After phase 2.2 the audio session
/// (queue, playback, library, MPV/PipeWire/Subsonic) is owned by
/// `DaemonCore`. `App` holds an `Arc<DaemonCore>` and runs the cava
/// subprocess + the TUI event loop. Phase 5 splits `App` into the
/// `ferrosonic` binary and `DaemonCore` into `ferrosonicd`.
pub struct App {
    /// Daemon-side core. Phase 2 keeps a direct handle for the lifecycle
    /// methods (`start_mpv`/`quit_mpv`) and the polling tick; every
    /// other touch should go through `client`. Phase 5 removes this
    /// when the daemon moves into its own process.
    pub(crate) core: Arc<DaemonCore>,
    /// Daemon command channel. In phase 2 this is an `InProcessClient`
    /// that dispatches directly to `core`; in phase 4 the same trait
    /// object becomes a `SocketClient` without any handler change.
    pub(crate) client: Arc<dyn DaemonClient>,
    /// Same `Arc<RwLock<AppState>>` that `core.state` wraps. Held here so
    /// render/input code reads/writes `state.client.X` without going
    /// through the core. Phase 6 replaces this with a client-side
    /// snapshot updated from `core.event_tx`.
    pub(crate) state: SharedState,
    /// Cava child process
    pub(crate) cava_process: Option<std::process::Child>,
    /// Cava pty master fd for reading output
    pub(crate) cava_pty_master: Option<std::fs::File>,
    /// Cava terminal parser
    pub(crate) cava_parser: Option<vt100::Parser>,
    /// Last mouse click position and time (for second-click detection)
    pub(crate) last_click: Option<(u16, u16, std::time::Instant)>,
    /// MPRIS D-Bus server
    pub(crate) mpris_server: Option<mpris_server::Server<MprisPlayer>>,
}

impl App {
    /// Create a new application instance. Builds the shared `AppState`,
    /// then constructs the `DaemonCore` against it. After this call the
    /// caller owns both `self.core` and `self.state` — they reference the
    /// same `Arc<RwLock<AppState>>` internally.
    pub fn new(config: Config) -> Self {
        let state = new_shared_state(config.clone());
        let core = DaemonCore::new(state.clone(), &config);
        let client: Arc<dyn DaemonClient> = Arc::new(InProcessClient::new(core.clone()));

        Self {
            core,
            client,
            state,
            cava_process: None,
            cava_pty_master: None,
            cava_parser: None,
            last_click: None,
            mpris_server: None,
        }
    }

    /// Run the application
    pub async fn run(&mut self) -> Result<(), Error> {
        // Start MPV via the daemon core
        if let Err(e) = self.core.start_mpv().await {
            warn!("Failed to start MPV: {} - audio playback won't work", e);
            let mut state = self.state.write().await;
            state
                .client
                .notify_error(format!("Failed to start MPV: {}. Is mpv installed?", e));
            drop(state);
        } else {
            info!("MPV started successfully, ready for playback");
        }

        // Start MPRIS server for media key support — passes the client
        // trait object so phase 4's SocketClient drops in unchanged.
        match start_mpris_server(self.state.clone(), self.client.clone()).await {
            Ok(server) => {
                info!("MPRIS server started");
                self.mpris_server = Some(server);
            }
            Err(e) => {
                warn!(
                    "Failed to start MPRIS server: {} — media keys won't work",
                    e
                );
            }
        }

        // Seed and load themes
        {
            use crate::ui::theme::{load_themes, seed_default_themes};
            if let Some(themes_dir) = crate::config::paths::themes_dir() {
                seed_default_themes(&themes_dir);
            }
            let themes = load_themes();
            let mut state = self.state.write().await;
            let theme_name = state.daemon.config.theme.clone();
            state.client.settings_state.themes = themes;
            state.client.settings_state.set_theme_by_name(&theme_name);
        }

        // Check if cava is available
        let cava_available = std::process::Command::new("which")
            .arg("cava")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);

        {
            let mut state = self.state.write().await;
            state.client.cava_available = cava_available;
            if !cava_available {
                state.client.settings_state.cava_enabled = false;
            }
        }

        // Start cava if enabled and available
        {
            let state = self.state.read().await;
            if state.client.settings_state.cava_enabled && cava_available {
                let td = state.client.settings_state.current_theme();
                let g = td.cava_gradient.clone();
                let h = td.cava_horizontal_gradient.clone();
                let cs = state.client.settings_state.cava_size as u32;
                drop(state);
                self.start_cava(&g, &h, cs);
            }
        }

        // Setup terminal
        enable_raw_mode().map_err(UiError::TerminalInit)?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture)
            .map_err(UiError::TerminalInit)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend).map_err(UiError::TerminalInit)?;

        info!("Terminal initialized");

        // Load initial data if configured
        {
            let has_client = self.core.subsonic.read().await.is_some();
            if has_client {
                self.load_initial_data().await;
            }
        }

        // Main event loop
        let result = self.event_loop(&mut terminal).await;

        // Cleanup cava
        self.stop_cava();

        // Cleanup MPV
        self.core.quit_mpv().await;

        // Cleanup terminal
        disable_raw_mode().map_err(UiError::TerminalInit)?;
        execute!(
            terminal.backend_mut(),
            LeaveAlternateScreen,
            DisableMouseCapture
        )
        .map_err(UiError::TerminalInit)?;
        terminal.show_cursor().map_err(UiError::Render)?;

        info!("Terminal restored");
        result
    }

    /// Cheap snapshot of the daemon's Subsonic client (`reqwest::Client`
    /// is Arc-wrapped internally). Returns `None` when not configured.
    /// Lets input/mouse handlers run direct API calls without holding a
    /// `RwLockReadGuard` across `.await` points. Phase 6 removes the
    /// remaining direct API call sites that motivate this accessor.
    pub(crate) async fn subsonic_client(&self) -> Option<crate::subsonic::SubsonicClient> {
        self.core.subsonic.read().await.clone()
    }

    /// Load initial data from server. Delegates the library fetches to
    /// `DaemonCore`; only the page-default selection is client state.
    pub(crate) async fn load_initial_data(&mut self) {
        {
            let mut state = self.state.write().await;
            state.client.songs.selected_option = Some(SongOption::Starred);
        }
        let _ = self.client.request(DaemonRequest::RefreshStarred).await;
        let _ = self.client.request(DaemonRequest::RefreshArtists).await;
        let _ = self.client.request(DaemonRequest::RefreshPlaylists).await;
    }

    /// Main event loop
    async fn event_loop(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    ) -> Result<(), Error> {
        let mut last_playback_update = std::time::Instant::now();

        loop {
            // Determine tick rate based on whether cava is active
            let cava_active = self.cava_parser.is_some();
            let tick_rate = if cava_active {
                Duration::from_millis(16) // ~60fps
            } else {
                Duration::from_millis(100)
            };

            // Draw UI
            {
                let mut state = self.state.write().await;
                terminal
                    .draw(|frame| ui::draw(frame, &mut state))
                    .map_err(UiError::Render)?;
            }

            // Check for quit
            {
                let state = self.state.read().await;
                if state.client.should_quit {
                    break;
                }
            }

            // Handle events with timeout
            if event::poll(tick_rate).map_err(UiError::Input)? {
                let event = event::read().map_err(UiError::Input)?;
                self.handle_event(event).await?;
            }

            // Read cava output (non-blocking)
            self.read_cava_output().await;

            // Update playback position every ~500ms. Phase 2.5 lifts this
            // into a `tokio::spawn`'d task on `core` itself; for now it's
            // driven from the event loop tick.
            let now = std::time::Instant::now();
            if now.duration_since(last_playback_update) >= Duration::from_millis(500) {
                last_playback_update = now;
                self.core.update_playback_info().await;
            }

            // Check for notification auto-clear (after 2 seconds)
            {
                let mut state = self.state.write().await;
                state.client.check_notification_timeout();
            }
        }

        Ok(())
    }
}
