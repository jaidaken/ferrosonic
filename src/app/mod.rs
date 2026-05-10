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
    /// Daemon-side core. `Some` for the in-process build (`App::new`)
    /// where `App` and `DaemonCore` co-own the same state lock; `None`
    /// for the split build (`App::with_remote_client`) where mpv lives
    /// in `ferrosonicd`. Lifecycle calls (`start_mpv`/`quit_mpv`/
    /// `spawn_polling_task`) only fire when `Some`.
    pub(crate) core: Option<Arc<DaemonCore>>,
    /// Daemon command channel. Either `InProcessClient` (in-process
    /// build) or `SocketClient` (split build) — the input/mouse
    /// handlers go through this trait either way.
    pub(crate) client: Arc<dyn DaemonClient>,
    /// `Arc<RwLock<AppState>>`. In the in-process build, the same Arc
    /// `core.state` wraps. In the split build, the App owns it solely
    /// — the `state.daemon` half is a mirror written by the event-pump
    /// task from `client.subscribe()`. Both render and input read it.
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
            core: Some(core),
            client,
            state,
            cava_process: None,
            cava_pty_master: None,
            cava_parser: None,
            last_click: None,
            mpris_server: None,
        }
    }

    /// Construct an App that talks to a remote daemon via `client`. No
    /// `DaemonCore` is built — mpv, the queue, and the library cache
    /// live in `ferrosonicd`. The TUI's `state.daemon` half is a mirror
    /// populated at boot from `DaemonRequest::Snapshot` and updated by
    /// the event-pump task spawned in `run()`.
    ///
    /// Used by the `ferrosonic` binary's split path. The in-process
    /// path keeps using `App::new(config)`.
    pub fn with_remote_client(client: Arc<dyn DaemonClient>, config: Config) -> Self {
        let state = new_shared_state(config);
        Self {
            core: None,
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
        // In-process build: spawn the daemon's playback poll + start
        // mpv here. Split build: ferrosonicd already did both; the TUI
        // only does view-side work.
        let _poll_task = self.core.as_ref().map(|c| c.spawn_polling_task());

        if let Some(ref core) = self.core {
            if let Err(e) = core.start_mpv().await {
                warn!("Failed to start MPV: {} - audio playback won't work", e);
                let mut state = self.state.write().await;
                state
                    .client
                    .notify_error(format!("Failed to start MPV: {}. Is mpv installed?", e));
                drop(state);
            } else {
                info!("MPV started successfully, ready for playback");
            }
        }

        // Split build: pull the initial state snapshot and start the
        // event-pump task that mirrors daemon events into state.daemon.
        if self.core.is_none() {
            self.bootstrap_remote_mirror().await;
            self.spawn_event_pump();
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

        // Load initial data if configured. In-process: directly check
        // the local SubsonicClient. Split: skip — ferrosonicd already
        // populated the library before the TUI connected (and the
        // snapshot at boot delivered it).
        if let Some(ref core) = self.core {
            let has_client = core.subsonic.read().await.is_some();
            if has_client {
                self.load_initial_data().await;
            }
        }

        // Main event loop
        let result = self.event_loop(&mut terminal).await;

        // Cleanup cava
        self.stop_cava();

        // Cleanup MPV (in-process only — ferrosonicd manages its own
        // lifecycle for the split build).
        if let Some(ref core) = self.core {
            core.quit_mpv().await;
        }

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

    /// Fetch an album's songs through the daemon. Works in both
    /// in-process and split-build modes. Returns an empty `Vec` on
    /// failure (the daemon logs + emits a notification event).
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

    /// Fetch a playlist's songs through the daemon. Same shape as
    /// `load_album`.
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

    /// Pull the initial daemon state snapshot via `Snapshot` request and
    /// install it as the local `state.daemon` mirror. Split build only.
    async fn bootstrap_remote_mirror(&self) {
        match self.client.request(DaemonRequest::Snapshot).await {
            Ok(crate::ipc::DaemonResponse::Snapshot(snap)) => {
                let mut state = self.state.write().await;
                state.daemon = *snap;
                info!(
                    "Daemon snapshot received: {} queue items, {} starred, {} artists, {} playlists",
                    state.daemon.queue.len(),
                    state.daemon.library.starred_songs.len(),
                    state.daemon.library.artists.len(),
                    state.daemon.library.playlists.len(),
                );
            }
            Ok(other) => {
                warn!("Unexpected snapshot response: {:?}", other);
            }
            Err(e) => {
                warn!("Failed to fetch daemon snapshot: {}", e);
            }
        }
    }

    /// Spawn the event-pump task. Subscribes to daemon events and
    /// applies them to `state.daemon` so the TUI render path sees the
    /// same data the daemon does. Split build only.
    fn spawn_event_pump(&self) {
        let client = self.client.clone();
        let state = self.state.clone();
        tokio::spawn(async move {
            let mut rx = client.subscribe();
            loop {
                match rx.recv().await {
                    Ok(ev) => apply_event(&state, ev).await,
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        warn!("Event pump lagged by {}; resubscribing + resnapshot", n);
                        // Resnapshot to recover from the lag.
                        if let Ok(crate::ipc::DaemonResponse::Snapshot(snap)) =
                            client.request(DaemonRequest::Snapshot).await
                        {
                            let mut s = state.write().await;
                            s.daemon = *snap;
                        }
                        rx = client.subscribe();
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        warn!("Daemon event broadcast closed; pump exiting");
                        break;
                    }
                }
            }
        });
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

    /// Main event loop. After phase 2.5 it does only TUI-side work:
    /// drawing, reading input, reading cava output, and notification
    /// timeout. Playback tracking runs on the daemon-side polling
    /// task spawned in `App::run`.
    async fn event_loop(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    ) -> Result<(), Error> {
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

            // Check for notification auto-clear (after 2 seconds)
            {
                let mut state = self.state.write().await;
                state.client.check_notification_timeout();
            }
        }

        Ok(())
    }
}

/// Apply one `DaemonEvent` to the local state mirror. Used by the
/// event-pump task in the split build. Each variant writes the
/// matching `state.daemon` slot; client-side state (`state.client`) is
/// only touched for `Notification` events.
async fn apply_event(state: &SharedState, ev: crate::ipc::DaemonEvent) {
    use crate::ipc::DaemonEvent;
    let mut s = state.write().await;
    match ev {
        DaemonEvent::QueueChanged { queue, position } => {
            s.daemon.queue = queue;
            s.daemon.queue_position = position;
        }
        DaemonEvent::NowPlayingChanged(np) => {
            s.daemon.now_playing = np;
        }
        DaemonEvent::PositionTick(pos) => {
            s.daemon.now_playing.position = pos;
        }
        DaemonEvent::StarredChanged(songs) => s.daemon.library.starred_songs = songs,
        DaemonEvent::RandomChanged(songs) => s.daemon.library.random_songs = songs,
        DaemonEvent::ArtistsChanged(artists) => s.daemon.library.artists = artists,
        DaemonEvent::AlbumsChanged { artist_id, albums } => {
            s.daemon.library.albums_cache.insert(artist_id, albums);
        }
        DaemonEvent::AlbumSongsChanged { album_id, songs } => {
            s.daemon.library.album_songs_cache.insert(album_id, songs);
        }
        DaemonEvent::PlaylistsChanged(playlists) => s.daemon.library.playlists = playlists,
        DaemonEvent::PlaylistSongsChanged { playlist_id, songs } => {
            s.daemon
                .library
                .playlist_songs_cache
                .insert(playlist_id, songs);
        }
        DaemonEvent::Notification { message, is_error } => {
            if is_error {
                s.client.notify_error(message);
            } else {
                s.client.notify(message);
            }
        }
        DaemonEvent::ConfigChanged(cfg) => {
            s.daemon.config = cfg;
        }
        DaemonEvent::Shutdown => {
            s.client.notify_error("Daemon shut down — disconnecting");
            s.client.should_quit = true;
        }
    }
}
