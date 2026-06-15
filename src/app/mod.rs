//! Main application module

pub mod cava_pipe;
pub mod client_state;
pub mod event_pump;
pub mod event_source;
mod input;
mod input_library;
mod input_playlists;
mod input_queue;
mod input_server;
mod input_settings;
mod input_songs;
pub mod lifecycle;
pub mod models;
mod mouse;
mod mouse_library;
mod mouse_playlists;
pub mod page_state;
pub mod spawn_daemon;
pub mod state;

use std::io;
use std::sync::Arc;
use std::time::Duration;

use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
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
use crate::mpris::server::{start_mpris_server, update_mpris_properties};
use crate::ui;

pub use event_pump::apply_event;
pub use lifecycle::{
    handle_signal_received, spawn_quit_listener, wait_for_unix_quit_signal, TerminalGuard,
};
pub use state::*;

/// The TUI application: daemon client, UI state, cava plumbing.
pub struct App {
    /// `Some` in-process, `None` when talking to a remote daemon.
    pub(crate) core: Option<Arc<DaemonCore>>,
    pub(crate) client: Arc<dyn DaemonClient>,
    /// Mirror of the daemon state, updated by events.
    pub daemon_state: SharedDaemonState,
    /// Client-local UI state.
    pub client_state: SharedClientState,
    pub(crate) cava_process: Option<std::process::Child>,
    pub(crate) cava_pty_master: Option<std::fs::File>,
    pub(crate) cava_parser: Option<vt100::Parser>,
    /// Holding the `NamedTempFile` keeps the cava config alive and
    /// removes it on drop / `stop_cava`.
    pub(crate) cava_config: Option<tempfile::NamedTempFile>,
    pub(crate) last_click: Option<(u16, u16, std::time::Instant)>,
    /// Guard must never span an .await; clippy::await_holding_lock enforces.
    pub(crate) cover_art: std::sync::Arc<std::sync::Mutex<crate::ui::cover_art::CoverArtState>>,
}

impl App {
    /// Standalone-mode constructor: daemon core runs in-process.
    pub fn new(config: Config) -> Self {
        let daemon_state = new_shared_daemon_state_with_restored_queue(config.clone());
        let client_state = new_shared_client_state(&config);
        let core = DaemonCore::new(daemon_state.clone(), &config);
        let client: Arc<dyn DaemonClient> = Arc::new(InProcessClient::new(core.clone()));

        Self {
            core: Some(core),
            client,
            daemon_state,
            client_state,
            cava_process: None,
            cava_pty_master: None,
            cava_parser: None,
            cava_config: None,
            last_click: None,
            cover_art: std::sync::Arc::new(std::sync::Mutex::new(
                crate::ui::cover_art::CoverArtState {
                    picker: None,
                    protocol_type: None,
                    cell_size: (10, 20),
                    current_id: None,
                    image: None,
                    protocol: None,
                    chafa_cache: None,
                },
            )),
        }
    }

    /// Split-build constructor. `state.daemon` is a mirror populated
    /// from `DaemonRequest::Snapshot` and the event pump.
    pub fn with_remote_client(client: Arc<dyn DaemonClient>, config: Config) -> Self {
        let daemon_state = new_shared_daemon_state(config.clone());
        let client_state = new_shared_client_state(&config);
        Self {
            core: None,
            client,
            daemon_state,
            client_state,
            cava_process: None,
            cava_pty_master: None,
            cava_parser: None,
            cava_config: None,
            last_click: None,
            cover_art: std::sync::Arc::new(std::sync::Mutex::new(
                crate::ui::cover_art::CoverArtState {
                    picker: None,
                    protocol_type: None,
                    cell_size: (10, 20),
                    current_id: None,
                    image: None,
                    protocol: None,
                    chafa_cache: None,
                },
            )),
        }
    }

    fn spawn_signal_quit(&self) {
        spawn_quit_listener(self.client_state.clone(), wait_for_unix_quit_signal());
    }

    /// Test seam: load themes and set the active one from daemon config.
    pub async fn load_and_apply_themes(&self) {
        use crate::ui::theme::{load_themes, seed_default_themes};
        if let Some(themes_dir) = crate::config::paths::themes_dir() {
            seed_default_themes(&themes_dir);
        }
        let themes = load_themes();
        let theme_name = {
            let ds = self.daemon_state.read().await;
            ds.config.theme.clone()
        };
        let mut cs = self.client_state.write().await;
        cs.settings_state.themes = themes;
        cs.settings_state.set_theme_by_name(&theme_name);
    }

    /// Test seam: check if cava binary is on PATH, update client_state.
    pub async fn probe_cava_available(&self) {
        let cava_available = std::process::Command::new("which")
            .arg("cava")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        let mut cs = self.client_state.write().await;
        cs.cava_available = cava_available;
        if !cava_available {
            cs.settings_state.cava_enabled = false;
        }
    }

    /// Test seam: start mpv inside the daemon core, notify on error.
    pub async fn start_mpv_with_notification(&self) {
        if let Some(ref core) = self.core {
            if let Err(e) = core.start_mpv().await {
                warn!("Failed to start MPV: {} - audio playback won't work", e);
                let mut cs = self.client_state.write().await;
                cs.notify_error(format!("Failed to start MPV: {}. Is mpv installed?", e));
            } else {
                info!("MPV started successfully, ready for playback");
            }
        }
    }

    /// Run the TUI event loop until quit.
    pub async fn run(&mut self) -> Result<(), Error> {
        // A remote daemon (core == None) outlives the TUI; gate the quit prompt.
        self.client_state.write().await.daemon_backed = self.core.is_none();
        self.spawn_signal_quit();
        let _term_guard = TerminalGuard::new_crossterm();
        let _poll_task = self.core.as_ref().map(|c| c.spawn_polling_task());

        self.start_mpv_with_notification().await;

        if self.core.is_none() {
            self.bootstrap_and_pump().await;
        }

        match start_mpris_server(
            self.daemon_state.clone(),
            self.client_state.clone(),
            self.client.clone(),
        )
        .await
        {
            Ok(server) => {
                info!("MPRIS server started");
                self.spawn_mpris_pump(server);
            }
            Err(e) => {
                warn!(
                    "Failed to start MPRIS server: {} — media keys won't work",
                    e
                );
            }
        }

        self.load_and_apply_themes().await;
        self.probe_cava_available().await;
        let cava_available = self.client_state.read().await.cava_available;

        {
            let cs = self.client_state.read().await;
            if cs.settings_state.cava_enabled && cava_available {
                let td = cs.settings_state.current_theme();
                let g = td.cava_gradient.clone();
                let h = td.cava_horizontal_gradient.clone();
                let size = cs.settings_state.cava_size as u32;
                drop(cs);
                self.start_cava(&g, &h, size);
            }
        }

        enable_raw_mode().map_err(UiError::TerminalInit)?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture)
            .map_err(UiError::TerminalInit)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend).map_err(UiError::TerminalInit)?;

        info!("Terminal initialized");

        {
            let probed = crate::ui::cover_art::CoverArtState::init();
            let mut guard = self.cover_art.lock().unwrap_or_else(|p| p.into_inner());
            *guard = probed;
        }

        // Snapshot-loaded current song needs an explicit cover-art fetch
        // now that the picker is initialised. (No NowPlayingChanged
        // event fires for an already-running daemon.)
        self.seed_cover_art().await;

        // Split-build: ferrosonicd has already populated the library
        // and the snapshot delivered it. In-process: fetch here.
        if let Some(ref core) = self.core {
            let has_client = core.subsonic.read().await.is_some();
            if has_client {
                self.load_initial_data().await;
            }
        }

        let result = self.event_loop(&mut terminal).await;

        self.shutdown_subsystems().await;

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

    /// Test seam: stop cava + quit mpv. Idempotent. Does not touch terminal.
    pub async fn shutdown_subsystems(&mut self) {
        self.stop_cava();
        if let Some(ref core) = self.core {
            core.quit_mpv().await;
        }
    }

    pub(crate) async fn load_album(&self, album_id: &str) -> Vec<crate::subsonic::models::Child> {
        match self
            .client
            .request(DaemonRequest::LoadAlbum(album_id.to_string()))
            .await
        {
            Ok(crate::ipc::DaemonResponse::AlbumSongs(songs)) => songs,
            _ => Vec::new(),
        }
    }

    pub(crate) async fn load_playlist(
        &self,
        playlist_id: &str,
    ) -> Vec<crate::subsonic::models::Child> {
        match self
            .client
            .request(DaemonRequest::LoadPlaylist(playlist_id.to_string()))
            .await
        {
            Ok(crate::ipc::DaemonResponse::PlaylistSongs(songs)) => songs,
            _ => Vec::new(),
        }
    }

    /// Prefetch cover art for the current song so first render has it.
    pub async fn seed_cover_art(&self) {
        let (id, enabled) = {
            let ds = self.daemon_state.read().await;
            let cs = self.client_state.read().await;
            (
                ds.now_playing
                    .song
                    .as_ref()
                    .and_then(|s| s.cover_art.clone()),
                cs.settings_state.cover_art,
            )
        };
        if !enabled {
            return;
        }
        let Some(id) = id else { return };
        info!("Seeding cover art for current song id={}", id);
        if let Ok(crate::ipc::DaemonResponse::CoverArt(bytes)) = self
            .client
            .request(DaemonRequest::FetchCoverArt {
                id: id.clone(),
                size: 512,
            })
            .await
        {
            if !bytes.is_empty() {
                let mut guard = self.cover_art.lock().unwrap_or_else(|p| p.into_inner());
                guard.load(id, &bytes);
            }
        }
    }

    /// Subscribe BEFORE Snapshot RPC so daemon events emitted during
    /// the RPC land in the receiver buffer instead of being lost
    /// (tokio broadcast only delivers events after subscribe).
    pub async fn bootstrap_and_pump(&self) {
        let rx = self.client.subscribe();

        let snap = match self.client.request(DaemonRequest::Snapshot).await {
            Ok(crate::ipc::DaemonResponse::Snapshot(s)) => Some(s),
            Ok(other) => {
                warn!("Unexpected snapshot response: {:?}", other);
                None
            }
            Err(e) => {
                warn!("Failed to fetch daemon snapshot: {}", e);
                None
            }
        };

        if let Some(snap) = snap {
            let (queue, queue_position) = {
                let mut ds = self.daemon_state.write().await;
                *ds = *snap;
                info!(
                    "Snapshot: queue={} starred={} artists={} playlists={}",
                    ds.queue.len(),
                    ds.library.starred_songs.len(),
                    ds.library.artists.len(),
                    ds.library.playlists.len(),
                );
                (ds.queue.clone(), ds.queue_position)
            };
            // Reopening mid-playback: default the Library right pane to the
            // playing album instead of leaving it blank until an album hover.
            if queue_position.is_some() {
                let mut cs = self.client_state.write().await;
                cs.artists.songs = queue;
                cs.artists.selected_song = queue_position;
            }
        }

        let daemon_state = self.daemon_state.clone();
        let client_state = self.client_state.clone();
        let client = self.client.clone();
        let cover_art = self.cover_art.clone();
        tokio::spawn(async move {
            event_pump::run_event_pump(client, daemon_state, client_state, cover_art, rx).await
        });
    }

    fn spawn_mpris_pump(&self, server: mpris_server::Server<crate::mpris::server::MprisPlayer>) {
        use crate::ipc::DaemonEvent;
        let mut rx = self.client.subscribe();
        let daemon_state = self.daemon_state.clone();
        tokio::spawn(async move {
            let server = server;
            loop {
                match rx.recv().await {
                    Ok(DaemonEvent::NowPlayingChanged(_))
                    | Ok(DaemonEvent::QueueChanged { .. }) => {
                        let _ = update_mpris_properties(&server, &daemon_state).await;
                    }
                    Ok(DaemonEvent::Shutdown) => break,
                    Ok(_) => {}
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        });
    }

    /// Request a snapshot plus initial library refreshes from the daemon.
    pub async fn load_initial_data(&mut self) {
        {
            let mut cs = self.client_state.write().await;
            cs.songs.selected_option = Some(SongOption::Starred);
        }
        let _ = self.client.request(DaemonRequest::RefreshStarred).await;
        let _ = self.client.request(DaemonRequest::RefreshArtists).await;
        let _ = self.client.request(DaemonRequest::RefreshPlaylists).await;
    }

    async fn event_loop(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    ) -> Result<(), Error> {
        let mut source = event_source::CrosstermEventSource;
        self.run_with_source(terminal, &mut source).await
    }

    /// Generic loop: any Backend + any EventSource. Tests use TestBackend + ChannelEventSource.
    pub async fn run_with_source<B, E>(
        &mut self,
        terminal: &mut Terminal<B>,
        source: &mut E,
    ) -> Result<(), Error>
    where
        B: ratatui::backend::Backend,
        E: event_source::EventSource,
    {
        // Paint the alt screen to the default bg; cells ratatui never writes
        // otherwise keep the terminal's blank screen, which renders black.
        terminal.clear().map_err(UiError::Render)?;
        loop {
            let tick_rate = self.tick_rate();
            self.draw_once(terminal).await?;
            if self.should_quit().await {
                break;
            }
            if let Some(ev) = source.next(tick_rate).await {
                let resized = matches!(ev, crossterm::event::Event::Resize(_, _));
                self.handle_event(ev).await?;
                if resized {
                    terminal.clear().map_err(UiError::Render)?;
                }
            }
            self.read_cava_output().await;
            self.tick_post().await;
        }
        Ok(())
    }

    fn tick_rate(&self) -> Duration {
        if self.cava_parser.is_some() {
            Duration::from_millis(16)
        } else {
            Duration::from_millis(100)
        }
    }

    /// Test seam: render one frame into any Backend (TestBackend in
    /// tests, CrosstermBackend in production).
    pub async fn draw_once<B: ratatui::backend::Backend>(
        &mut self,
        terminal: &mut Terminal<B>,
    ) -> Result<(), Error> {
        let ds = self.daemon_state.read().await;
        let mut cs = self.client_state.write().await;
        let mut bundle = AppState {
            daemon: &ds,
            client: &mut cs,
        };
        let cover_art = self.cover_art.clone();
        terminal
            .draw(|frame| ui::draw(frame, &mut bundle, &cover_art))
            .map_err(UiError::Render)?;
        Ok(())
    }

    /// Test seam: check the quit flag.
    pub async fn should_quit(&self) -> bool {
        self.client_state.read().await.should_quit
    }

    /// Test seam: per-tick post-event work (notification expiry).
    pub async fn tick_post(&mut self) {
        let mut cs = self.client_state.write().await;
        cs.check_notification_timeout();
    }
}

