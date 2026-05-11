//! Main application module

pub mod cava;
pub mod client_state;
mod input;
mod input_library;
mod input_playlists;
mod input_queue;
mod input_server;
mod input_settings;
mod input_songs;
pub mod models;
mod mouse;
mod mouse_library;
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

pub struct App {
    /// `Some` in-process, `None` when talking to a remote daemon.
    pub(crate) core: Option<Arc<DaemonCore>>,
    pub(crate) client: Arc<dyn DaemonClient>,
    pub daemon_state: SharedDaemonState,
    pub client_state: SharedClientState,
    pub(crate) cava_process: Option<std::process::Child>,
    pub(crate) cava_pty_master: Option<std::fs::File>,
    pub(crate) cava_parser: Option<vt100::Parser>,
    /// Holding the `NamedTempFile` keeps the cava config alive and
    /// removes it on drop / `stop_cava`.
    pub(crate) cava_config: Option<tempfile::NamedTempFile>,
    pub(crate) last_click: Option<(u16, u16, std::time::Instant)>,
    /// `picker` is `None` until `run()` probes the terminal.
    pub(crate) cover_art: std::sync::Arc<std::sync::Mutex<crate::ui::cover_art::CoverArtState>>,
}

impl App {
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
        let client_state = self.client_state.clone();
        tokio::spawn(async move {
            use tokio::signal::unix::{signal, SignalKind};
            let mut term = match signal(SignalKind::terminate()) {
                Ok(s) => s,
                Err(_) => return,
            };
            let mut int = match signal(SignalKind::interrupt()) {
                Ok(s) => s,
                Err(_) => return,
            };
            let mut hup = match signal(SignalKind::hangup()) {
                Ok(s) => s,
                Err(_) => return,
            };
            tokio::select! {
                _ = term.recv() => {}
                _ = int.recv() => {}
                _ = hup.recv() => {}
            }
            let mut s = client_state.write().await;
            s.should_quit = true;
        });
    }

    pub async fn run(&mut self) -> Result<(), Error> {
        self.spawn_signal_quit();
        let _term_guard = TerminalGuard;
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
            let mut guard = self.cover_art.lock().expect("cover_art poisoned");
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

        self.stop_cava();

        if let Some(ref core) = self.core {
            core.quit_mpv().await;
        }

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
                let mut guard = self.cover_art.lock().expect("cover_art poisoned");
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
        let cover_art = self.cover_art.clone();
        tokio::spawn(async move {
            run_event_pump(client, daemon_state, client_state, cover_art, rx).await
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

    pub(crate) async fn load_initial_data(&mut self) {
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
        loop {
            let cava_active = self.cava_parser.is_some();
            let tick_rate = if cava_active {
                Duration::from_millis(16)
            } else {
                Duration::from_millis(100)
            };

            {
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
            }

            {
                let cs = self.client_state.read().await;
                if cs.should_quit {
                    break;
                }
            }

            if event::poll(tick_rate).map_err(UiError::Input)? {
                let event = event::read().map_err(UiError::Input)?;
                self.handle_event(event).await?;
            }

            self.read_cava_output().await;

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
    cover_art: std::sync::Arc<std::sync::Mutex<crate::ui::cover_art::CoverArtState>>,
    mut rx: tokio::sync::broadcast::Receiver<crate::ipc::DaemonEvent>,
) {
    loop {
        match rx.recv().await {
            Ok(ev) => apply_event(&daemon_state, &client_state, &client, &cover_art, ev).await,
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

/// Lock order: daemon, then client. Same everywhere — avoids deadlock.
async fn apply_event(
    daemon_state: &SharedDaemonState,
    client_state: &SharedClientState,
    client: &Arc<dyn DaemonClient>,
    cover_art: &std::sync::Arc<std::sync::Mutex<crate::ui::cover_art::CoverArtState>>,
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
            let new_cover_id = np.song.as_ref().and_then(|s| s.cover_art.clone());
            let cover_art_enabled = {
                let cs = client_state.read().await;
                cs.settings_state.cover_art
            };
            {
                let mut ds = daemon_state.write().await;
                ds.now_playing = np;
            }
            if cover_art_enabled {
                if let Some(id) = new_cover_id {
                    let already_loaded = {
                        let guard = cover_art.lock().expect("cover_art poisoned");
                        guard.current_id.as_deref() == Some(id.as_str())
                    };
                    if !already_loaded {
                        info!("Fetching cover art id={}", id);
                        match client
                            .request(DaemonRequest::FetchCoverArt {
                                id: id.clone(),
                                size: 512,
                            })
                            .await
                        {
                            Ok(crate::ipc::DaemonResponse::CoverArt(bytes)) => {
                                info!("Cover art bytes received: {} bytes", bytes.len());
                                if !bytes.is_empty() {
                                    let mut guard = cover_art.lock().expect("cover_art poisoned");
                                    guard.load(id, &bytes);
                                }
                            }
                            Ok(other) => {
                                warn!("FetchCoverArt: unexpected response: {:?}", other);
                            }
                            Err(e) => {
                                warn!("FetchCoverArt failed: {}", e);
                            }
                        }
                    }
                } else {
                    let mut guard = cover_art.lock().expect("cover_art poisoned");
                    guard.clear();
                }
            }
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
            {
                let mut ds = daemon_state.write().await;
                for song in ds.queue.iter_mut() {
                    update(song);
                }
                for song in ds.library.random_songs.iter_mut() {
                    update(song);
                }
                for list in ds.library.album_songs_cache.values_mut() {
                    for song in list.iter_mut() {
                        update(song);
                    }
                }
                for list in ds.library.playlist_songs_cache.values_mut() {
                    for song in list.iter_mut() {
                        update(song);
                    }
                }
                if let Some(np) = ds.now_playing.song.as_mut() {
                    if np.id == id {
                        np.starred = marker.clone();
                    }
                }
            }
            {
                let mut cs = client_state.write().await;
                for song in cs.artists.songs.iter_mut() {
                    update(song);
                }
                for song in cs.playlists.songs.iter_mut() {
                    update(song);
                }
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
            let repeat_mode = cfg.repeat_mode;
            let cover_art_enabled = cfg.cover_art;
            let cover_art_size = cfg.cover_art_size;
            let auto_continue = cfg.auto_continue;
            {
                let mut ds = daemon_state.write().await;
                ds.config = cfg;
            }
            {
                let mut cs = client_state.write().await;
                cs.settings_state.repeat_mode = repeat_mode;
                cs.settings_state.cover_art = cover_art_enabled;
                cs.settings_state.cover_art_size = cover_art_size;
                cs.settings_state.auto_continue = auto_continue;
            }

            if cover_art_enabled {
                let (current_id, already_loaded) = {
                    let ds = daemon_state.read().await;
                    let guard = cover_art.lock().expect("cover_art poisoned");
                    let id = ds
                        .now_playing
                        .song
                        .as_ref()
                        .and_then(|s| s.cover_art.clone());
                    let loaded = match (&id, &guard.current_id) {
                        (Some(i), Some(c)) => i == c,
                        _ => false,
                    };
                    (id, loaded)
                };
                if let Some(id) = current_id {
                    if !already_loaded {
                        info!("Cover art enabled; fetching current id={}", id);
                        if let Ok(crate::ipc::DaemonResponse::CoverArt(bytes)) = client
                            .request(DaemonRequest::FetchCoverArt {
                                id: id.clone(),
                                size: 512,
                            })
                            .await
                        {
                            if !bytes.is_empty() {
                                let mut guard = cover_art.lock().expect("cover_art poisoned");
                                guard.load(id, &bytes);
                            }
                        }
                    }
                }
            } else {
                let mut guard = cover_art.lock().expect("cover_art poisoned");
                guard.clear();
            }
        }
        DaemonEvent::RepeatModeChanged(mode) => {
            {
                let mut ds = daemon_state.write().await;
                ds.config.repeat_mode = mode;
            }
            let mut cs = client_state.write().await;
            cs.settings_state.repeat_mode = mode;
        }
        DaemonEvent::Shutdown => {
            let mut cs = client_state.write().await;
            cs.notify_error("Daemon shut down — disconnecting");
            cs.should_quit = true;
        }
    }
}
