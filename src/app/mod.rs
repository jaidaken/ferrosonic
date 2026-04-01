//! Main application module

pub mod actions;
mod cava;
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
mod playback;
mod repo;
pub mod state;

use std::io;
use std::time::Duration;

use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use tokio::sync::mpsc;
use tracing::{error, info, warn};

use crate::app::models::SongOption;
use crate::audio::mpv::MpvController;
use crate::audio::pipewire::PipeWireController;
use crate::config::Config;
use crate::error::{Error, UiError};
use crate::mpris::server::{start_mpris_server, update_mpris_properties, MprisPlayer};
use crate::subsonic::SubsonicClient;
use crate::ui;

pub use actions::*;
pub use state::*;

/// Channel buffer size
const CHANNEL_SIZE: usize = 256;

/// Main application
pub struct App {
    /// Shared application state
    state: SharedState,
    /// Subsonic client
    subsonic: Option<SubsonicClient>,
    /// MPV audio controller
    mpv: MpvController,
    /// PipeWire sample rate controller
    pipewire: PipeWireController,
    /// Channel to send audio actions
    audio_tx: mpsc::Sender<AudioAction>,
    /// Cava child process
    cava_process: Option<std::process::Child>,
    /// Cava pty master fd for reading output
    cava_pty_master: Option<std::fs::File>,
    /// Cava terminal parser
    cava_parser: Option<vt100::Parser>,
    /// Last mouse click position and time (for second-click detection)
    last_click: Option<(u16, u16, std::time::Instant)>,
    /// Channel to receive audio actions (from MPRIS)
    audio_rx: mpsc::Receiver<AudioAction>,
    /// MPRIS D-Bus server
    mpris_server: Option<mpris_server::Server<MprisPlayer>>,
}

impl App {
    /// Create a new application instance
    pub fn new(config: Config) -> Self {
        let (audio_tx, audio_rx) = mpsc::channel(CHANNEL_SIZE);

        let state = new_shared_state(config.clone());

        let subsonic = if config.is_configured() {
            match SubsonicClient::new(&config.base_url, &config.username, &config.password) {
                Ok(client) => Some(client),
                Err(e) => {
                    warn!("Failed to create Subsonic client: {}", e);
                    None
                }
            }
        } else {
            None
        };

        Self {
            state,
            subsonic,
            mpv: MpvController::new(),
            pipewire: PipeWireController::new(),
            audio_tx,
            cava_process: None,
            cava_pty_master: None,
            cava_parser: None,
            last_click: None,
            audio_rx,
            mpris_server: None,
        }
    }

    /// Run the application
    pub async fn run(&mut self) -> Result<(), Error> {
        // Start MPV
        if let Err(e) = self.mpv.start() {
            warn!("Failed to start MPV: {} - audio playback won't work", e);
            let mut state = self.state.write().await;
            state.notify_error(format!("Failed to start MPV: {}. Is mpv installed?", e));
            drop(state);
        } else {
            info!("MPV started successfully, ready for playback");
        }

        // Start MPRIS server for media key support
        match start_mpris_server(self.state.clone(), self.audio_tx.clone()).await {
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
            let theme_name = state.config.theme.clone();
            state.settings_state.themes = themes;
            state.settings_state.set_theme_by_name(&theme_name);
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
            state.cava_available = cava_available;
            if !cava_available {
                state.settings_state.cava_enabled = false;
            }
        }

        // Start cava if enabled and available
        {
            let state = self.state.read().await;
            if state.settings_state.cava_enabled && cava_available {
                let td = state.settings_state.current_theme();
                let g = td.cava_gradient.clone();
                let h = td.cava_horizontal_gradient.clone();
                let cs = state.settings_state.cava_size as u32;
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
        if self.subsonic.is_some() {
            self.load_initial_data().await;
        }

        // Main event loop
        let result = self.event_loop(&mut terminal).await;

        // Cleanup cava
        self.stop_cava();

        // Cleanup MPV
        let _ = self.mpv.quit();

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

    /// Load initial data from server
    async fn load_initial_data(&mut self) {
        let mut state = self.state.write().await;
        state.songs.selected_option = Some(SongOption::Starred);
        drop(state);

        self.get_starred_songs().await;
        self.get_artists().await;
        self.get_playlists().await;
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
                if state.should_quit {
                    break;
                }
            }

            // Handle events with timeout
            if event::poll(tick_rate).map_err(UiError::Input)? {
                let event = event::read().map_err(UiError::Input)?;
                self.handle_event(event).await?;
            }

            // Process any pending audio actions (from MPRIS)
            while let Ok(action) = self.audio_rx.try_recv() {
                match action {
                    AudioAction::TogglePause => {
                        let _ = self.toggle_pause().await;
                    }
                    AudioAction::Pause => {
                        let _ = self.pause_playback().await;
                    }
                    AudioAction::Resume => {
                        let _ = self.resume_playback().await;
                    }
                    AudioAction::Next => {
                        let _ = self.next_track().await;
                    }
                    AudioAction::Previous => {
                        let _ = self.prev_track().await;
                    }
                    AudioAction::Stop => {
                        let _ = self.stop_playback().await;
                    }
                    AudioAction::Seek(pos) => {
                        if let Err(e) = self.mpv.seek(pos) {
                            warn!("MPRIS seek failed: {}", e);
                        } else {
                            let mut state = self.state.write().await;
                            state.now_playing.position = pos;
                        }
                    }
                    AudioAction::SeekRelative(offset) => {
                        let _ = self.mpv.seek_relative(offset);
                    }
                    AudioAction::SetVolume(vol) => {
                        let _ = self.mpv.set_volume(vol);
                    }
                }
            }

            // Read cava output (non-blocking)
            self.read_cava_output().await;

            // Update playback position every ~500ms
            let now = std::time::Instant::now();
            if now.duration_since(last_playback_update) >= Duration::from_millis(500) {
                last_playback_update = now;
                self.update_playback_info().await;
            }

            // Check for notification auto-clear (after 2 seconds)
            {
                let mut state = self.state.write().await;
                state.check_notification_timeout();
            }
        }

        Ok(())
    }
}
