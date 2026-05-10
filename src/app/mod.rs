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
use crate::mpris::server::{start_mpris_server, update_mpris_properties};
use crate::ui;

pub use state::*;

/// Main application — TUI client side. After phase 2.2 the audio session
/// (queue, playback, library, MPV/PipeWire/Subsonic) is owned by
/// `DaemonCore`. `App` holds an `Arc<DaemonCore>` and runs the cava
/// subprocess + the TUI event loop. Phase 5 splits `App` into the
/// `ferrosonic` binary and `DaemonCore` into `ferrosonicd`.
pub struct App {
    /// Daemon-side core. `Some` for the in-process build, `None` for
    /// the split build (mpv lives in `ferrosonicd`). Lifecycle calls
    /// (`start_mpv`/`quit_mpv`/`spawn_polling_task`) only fire when
    /// `Some`.
    pub(crate) core: Option<Arc<DaemonCore>>,
    /// Daemon command channel. `InProcessClient` or `SocketClient`.
    pub(crate) client: Arc<dyn DaemonClient>,
    /// Daemon-side state mirror. In-process: same Arc that `core.state`
    /// wraps. Split: TUI-local mirror, written by the event pump.
    pub(crate) daemon_state: SharedDaemonState,
    /// Client-side state: page, selection, scroll offsets, notifications,
    /// cava buffer, layout cache. Always owned solely by `App`.
    pub(crate) client_state: SharedClientState,
    /// Cava child process
    pub(crate) cava_process: Option<std::process::Child>,
    /// Cava pty master fd for reading output
    pub(crate) cava_pty_master: Option<std::fs::File>,
    /// Cava terminal parser
    pub(crate) cava_parser: Option<vt100::Parser>,
    /// Cava config file. Holding the `NamedTempFile` keeps the file
    /// alive for the duration of the cava process and removes it on
    /// drop / `stop_cava`.
    pub(crate) cava_config: Option<tempfile::NamedTempFile>,
    /// Last mouse click position and time (for second-click detection)
    pub(crate) last_click: Option<(u16, u16, std::time::Instant)>,
}

impl App {
    /// Create a new application instance. Builds the shared `AppState`,
    /// then constructs the `DaemonCore` against it. After this call the
    /// caller owns both `self.core` and `self.state` — they reference the
    /// same `Arc<RwLock<AppState>>` internally.
    pub fn new(config: Config) -> Self {
        let daemon_state = new_shared_daemon_state(config.clone());
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
        }
    }

    fn spawn_signal_quit(&self) {
        let client_state = self.client_state.clone();
        tokio::spawn(async move {
            use tokio::signal::unix::{signal, SignalKind};
            let mut term = match signal(SignalKind::terminate()) { Ok(s) => s, Err(_) => return };
            let mut int = match signal(SignalKind::interrupt()) { Ok(s) => s, Err(_) => return };
            let mut hup = match signal(SignalKind::hangup()) { Ok(s) => s, Err(_) => return };
            tokio::select! {
                _ = term.recv() => {}
                _ = int.recv() => {}
                _ = hup.recv() => {}
            }
            let mut s = client_state.write().await;
            s.should_quit = true;
        });
    }

    /// Run the application
    pub async fn run(&mut self) -> Result<(), Error> {
        self.spawn_signal_quit();
        let _term_guard = TerminalGuard;
        // In-process build: spawn the daemon's playback poll + start
        // mpv here. Split build: ferrosonicd already did both; the TUI
        // only does view-side work.
        let _poll_task = self.core.as_ref().map(|c| c.spawn_polling_task());

        if let Some(ref core) = self.core {
            if let Err(e) = core.start_mpv().await {
                warn!("Failed to start MPV: {} - audio playback won't work", e);
                let mut cs = self.client_state.write().await;
                cs.notify_error(format!("Failed to start MPV: {}. Is mpv installed?", e));
            } else {
                info!("MPV started successfully, ready for playback");
            }
        }

        if self.core.is_none() {
            self.bootstrap_and_pump().await;
        }

        // Start MPRIS server. Reads `daemon_state` for properties +
        // metadata; writes `client_state.should_quit` on MPRIS Quit.
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

        // Seed and load themes
        {
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

        // Check if cava is available
        let cava_available = std::process::Command::new("which")
            .arg("cava")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);

        {
            let mut cs = self.client_state.write().await;
            cs.cava_available = cava_available;
            if !cava_available {
                cs.settings_state.cava_enabled = false;
            }
        }

        // Start cava if enabled and available
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

    /// Subscribe to events first, then fetch the snapshot, then spawn
    /// the pump. Subscribing first means any events the daemon emits
    /// during the snapshot RPC are buffered in the receiver instead of
    /// dropped (tokio broadcast only delivers events sent after
    /// subscribe). The pump applies the snapshot then drains the
    /// buffered events normally.
    async fn bootstrap_and_pump(&self) {
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
            let mut ds = self.daemon_state.write().await;
            *ds = *snap;
            info!(
                "Snapshot: queue={} starred={} artists={} playlists={}",
                ds.queue.len(),
                ds.library.starred_songs.len(),
                ds.library.artists.len(),
                ds.library.playlists.len(),
            );
        }

        let daemon_state = self.daemon_state.clone();
        let client_state = self.client_state.clone();
        let client = self.client.clone();
        tokio::spawn(async move {
            run_event_pump(client, daemon_state, client_state, rx).await
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


    /// Load initial data from server. Delegates the library fetches to
    /// `DaemonCore`; only the page-default selection is client state.
    pub(crate) async fn load_initial_data(&mut self) {
        {
            let mut cs = self.client_state.write().await;
            cs.songs.selected_option = Some(SongOption::Starred);
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

            // Draw UI — read on daemon, write on client. Daemon poll
            // and other readers run in parallel with render.
            {
                let ds = self.daemon_state.read().await;
                let mut cs = self.client_state.write().await;
                let mut bundle = AppState {
                    daemon: &*ds,
                    client: &mut *cs,
                };
                terminal
                    .draw(|frame| ui::draw(frame, &mut bundle))
                    .map_err(UiError::Render)?;
            }

            // Check for quit
            {
                let cs = self.client_state.read().await;
                if cs.should_quit {
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
                let mut cs = self.client_state.write().await;
                cs.check_notification_timeout();
            }
        }

        Ok(())
    }
}

struct TerminalGuard;

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = crossterm::terminal::disable_raw_mode();
        let _ = crossterm::execute!(
            std::io::stdout(),
            crossterm::terminal::LeaveAlternateScreen,
            crossterm::event::DisableMouseCapture
        );
    }
}

async fn run_event_pump(
    client: Arc<dyn DaemonClient>,
    daemon_state: SharedDaemonState,
    client_state: SharedClientState,
    mut rx: tokio::sync::broadcast::Receiver<crate::ipc::DaemonEvent>,
) {
    loop {
        match rx.recv().await {
            Ok(ev) => apply_event(&daemon_state, &client_state, ev).await,
            Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                warn!("Event pump lagged by {}; resnapshot + resubscribe", n);
                let new_rx = client.subscribe();
                if let Ok(crate::ipc::DaemonResponse::Snapshot(snap)) =
                    client.request(DaemonRequest::Snapshot).await
                {
                    let mut ds = daemon_state.write().await;
                    *ds = *snap;
                }
                rx = new_rx;
            }
            Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                warn!("Daemon event broadcast closed; pump exiting");
                break;
            }
        }
    }
}

/// Applies a `DaemonEvent` to local state. Acquires locks in the same
/// order everywhere (daemon first, then client when needed) to avoid
/// deadlock with other writers.
async fn apply_event(
    daemon_state: &SharedDaemonState,
    client_state: &SharedClientState,
    ev: crate::ipc::DaemonEvent,
) {
    use crate::ipc::DaemonEvent;
    match ev {
        DaemonEvent::QueueChanged { queue, position } => {
            let mut ds = daemon_state.write().await;
            ds.queue = queue;
            ds.queue_position = position;
        }
        DaemonEvent::NowPlayingChanged(np) => {
            let mut ds = daemon_state.write().await;
            ds.now_playing = np;
        }
        DaemonEvent::PositionTick(pos) => {
            let mut ds = daemon_state.write().await;
            ds.now_playing.position = pos;
        }
        DaemonEvent::StarredChanged(songs) => {
            let mut ds = daemon_state.write().await;
            ds.library.starred_songs = songs;
        }
        DaemonEvent::SongStarChanged { id, starred } => {
            let marker = if starred { Some("1".to_string()) } else { None };
            let update = |song: &mut crate::subsonic::models::Child| {
                if song.id == id {
                    song.starred = marker.clone();
                }
            };
            // Daemon-side mirror first.
            {
                let mut ds = daemon_state.write().await;
                for song in ds.queue.iter_mut() { update(song); }
                for song in ds.library.random_songs.iter_mut() { update(song); }
                for list in ds.library.album_songs_cache.values_mut() {
                    for song in list.iter_mut() { update(song); }
                }
                for list in ds.library.playlist_songs_cache.values_mut() {
                    for song in list.iter_mut() { update(song); }
                }
                if let Some(np) = ds.now_playing.song.as_mut() {
                    if np.id == id { np.starred = marker.clone(); }
                }
            }
            // Then client-side per-page song caches.
            {
                let mut cs = client_state.write().await;
                for song in cs.artists.songs.iter_mut() { update(song); }
                for song in cs.playlists.songs.iter_mut() { update(song); }
            }
        }
        DaemonEvent::RandomChanged(songs) => {
            let mut ds = daemon_state.write().await;
            ds.library.random_songs = songs;
        }
        DaemonEvent::ArtistsChanged(artists) => {
            let mut ds = daemon_state.write().await;
            ds.library.artists = artists;
        }
        DaemonEvent::AlbumsChanged { artist_id, albums } => {
            let mut ds = daemon_state.write().await;
            ds.library.albums_cache.insert(artist_id, albums);
        }
        DaemonEvent::AlbumSongsChanged { album_id, songs } => {
            let mut ds = daemon_state.write().await;
            ds.library.album_songs_cache.insert(album_id, songs);
        }
        DaemonEvent::PlaylistsChanged(playlists) => {
            let mut ds = daemon_state.write().await;
            ds.library.playlists = playlists;
        }
        DaemonEvent::PlaylistSongsChanged { playlist_id, songs } => {
            let mut ds = daemon_state.write().await;
            ds.library.playlist_songs_cache.insert(playlist_id, songs);
        }
        DaemonEvent::Notification { message, is_error } => {
            let mut cs = client_state.write().await;
            if is_error {
                cs.notify_error(message);
            } else {
                cs.notify(message);
            }
        }
        DaemonEvent::ConfigChanged(cfg) => {
            let mut ds = daemon_state.write().await;
            ds.config = cfg;
        }
        DaemonEvent::Shutdown => {
            let mut cs = client_state.write().await;
            cs.notify_error("Daemon shut down — disconnecting");
            cs.should_quit = true;
        }
    }
}
