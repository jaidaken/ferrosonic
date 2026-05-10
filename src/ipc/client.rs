//! Client interface for talking to the daemon.
//!
//! Implementations:
//! - [`InProcessClient`] — dispatches directly to an `Arc<DaemonCore>`.
//!   Used by the single-binary build today and the in-tree tests.
//! - `SocketClient` (phase 4) — round-trips requests over a Unix domain
//!   socket. Same trait, no call-site changes when phase 5 splits the
//!   binaries.
//!
//! Why a trait at all, this early? The TUI's input/mouse handlers need
//! to call into the daemon thousands of times. By going through this
//! trait now (phase 2.3) we can swap in `SocketClient` in phase 4
//! without touching any handler. The cost is one async-fn-in-trait
//! indirection per call, which is negligible compared to the network
//! round-trip the socket version will introduce.

#![allow(dead_code)] // wired into App in phase 2.4

use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::broadcast;
use tracing::warn;

use crate::daemon::DaemonCore;
use crate::ipc::protocol::{
    DaemonEvent, DaemonRequest, DaemonResponse, EnqueueMode, IpcError,
};

/// The TUI client's view of the daemon. Every command goes through
/// `request`, every state-change subscription through `subscribe`.
///
/// Implementations must be `Send + Sync` so the trait can be stored
/// behind an `Arc<dyn DaemonClient>` and shared across spawned tasks.
#[async_trait]
pub trait DaemonClient: Send + Sync {
    /// Send a command to the daemon and await its reply.
    async fn request(&self, req: DaemonRequest) -> Result<DaemonResponse, IpcError>;

    /// Subscribe to the daemon's event broadcast. The returned receiver
    /// observes every event from the moment of subscription onward.
    /// Slow consumers may see `RecvError::Lagged`; they should drop the
    /// receiver and resubscribe (which the upcoming `event_pump` task in
    /// `App` does automatically).
    fn subscribe(&self) -> broadcast::Receiver<DaemonEvent>;
}

/// In-process implementation: a thin dispatch layer over `DaemonCore`.
///
/// All requests run on the caller's task — there is no message queue or
/// background worker. This matches today's single-binary architecture
/// exactly; the trait boundary exists so that phase 4's `SocketClient`
/// can drop in without changing any handler.
pub struct InProcessClient {
    core: Arc<DaemonCore>,
}

impl InProcessClient {
    pub fn new(core: Arc<DaemonCore>) -> Self {
        Self { core }
    }

    /// Underlying core access. Phase 2 only — call sites that still
    /// need direct core access during the migration use this. Phase 6
    /// removes it; everything must go through `request()`/`subscribe()`.
    pub fn core(&self) -> &Arc<DaemonCore> {
        &self.core
    }
}

#[async_trait]
impl DaemonClient for InProcessClient {
    async fn request(&self, req: DaemonRequest) -> Result<DaemonResponse, IpcError> {
        match req {
            // ── Audio control ───────────────────────────────────────
            DaemonRequest::Pause => {
                self.core.pause_playback().await.map_err(err)?;
                Ok(DaemonResponse::Ok)
            }
            DaemonRequest::Resume => {
                self.core.resume_playback().await.map_err(err)?;
                Ok(DaemonResponse::Ok)
            }
            DaemonRequest::TogglePause => {
                self.core.toggle_pause().await.map_err(err)?;
                Ok(DaemonResponse::Ok)
            }
            DaemonRequest::Stop => {
                self.core.stop_playback().await.map_err(err)?;
                Ok(DaemonResponse::Ok)
            }
            DaemonRequest::Seek(pos) => {
                self.core.seek(pos).await.map_err(err)?;
                Ok(DaemonResponse::Ok)
            }
            DaemonRequest::SeekRelative(off) => {
                self.core.seek_relative(off).await.map_err(err)?;
                Ok(DaemonResponse::Ok)
            }
            DaemonRequest::Next => {
                self.core.next_track().await.map_err(err)?;
                Ok(DaemonResponse::Ok)
            }
            DaemonRequest::Previous => {
                self.core.prev_track().await.map_err(err)?;
                Ok(DaemonResponse::Ok)
            }
            DaemonRequest::SetVolume(v) => {
                self.core.set_volume(v).await.map_err(err)?;
                Ok(DaemonResponse::Ok)
            }

            // ── Queue operations ────────────────────────────────────
            DaemonRequest::EnqueueSongs { songs, mode } => {
                self.enqueue_songs(songs, mode).await
            }
            DaemonRequest::PlayQueueIndex(pos) => {
                self.core.play_queue_position(pos).await.map_err(err)?;
                Ok(DaemonResponse::Ok)
            }
            DaemonRequest::RemoveFromQueue(pos) => {
                let mut state = self.core.state.write().await;
                if pos < state.daemon.queue.len() {
                    state.daemon.queue.remove(pos);
                    if let Some(cur) = state.daemon.queue_position {
                        if pos < cur {
                            state.daemon.queue_position = Some(cur - 1);
                        } else if pos == cur {
                            state.daemon.queue_position = None;
                        }
                    }
                }
                drop(state);
                self.core.broadcast_queue_changed().await;
                Ok(DaemonResponse::Ok)
            }
            DaemonRequest::ClearQueue => {
                self.core.stop_playback().await.map_err(err)?;
                Ok(DaemonResponse::Ok)
            }
            DaemonRequest::ShuffleQueue => {
                self.core.shuffle_queue().await;
                Ok(DaemonResponse::Ok)
            }
            DaemonRequest::MoveQueueItem { from, to } => {
                self.core.move_queue_item(from, to).await;
                Ok(DaemonResponse::Ok)
            }
            DaemonRequest::ClearQueueHistory => {
                let removed = self.core.clear_queue_history().await;
                Ok(DaemonResponse::HistoryCleared(removed))
            }

            // ── Library operations ──────────────────────────────────
            DaemonRequest::RefreshStarred => {
                self.core.refresh_starred().await;
                Ok(DaemonResponse::Ok)
            }
            DaemonRequest::RefreshRandom => {
                self.core.refresh_random().await;
                Ok(DaemonResponse::Ok)
            }
            DaemonRequest::RefreshArtists => {
                self.core.refresh_artists().await;
                Ok(DaemonResponse::Ok)
            }
            DaemonRequest::RefreshPlaylists => {
                self.core.refresh_playlists().await;
                Ok(DaemonResponse::Ok)
            }
            DaemonRequest::LoadArtist(id) => {
                self.core.load_artist(&id).await;
                let state = self.core.state.read().await;
                let albums = state
                    .daemon
                    .library
                    .albums_cache
                    .get(&id)
                    .cloned()
                    .unwrap_or_default();
                Ok(DaemonResponse::ArtistAlbums(albums))
            }
            DaemonRequest::LoadAlbum(id) => {
                let songs = self.core.load_album_songs(&id).await;
                Ok(DaemonResponse::AlbumSongs(songs))
            }
            DaemonRequest::LoadPlaylist(id) => {
                let songs = self.core.load_playlist_songs(&id).await;
                Ok(DaemonResponse::PlaylistSongs(songs))
            }

            // ── Config operations ───────────────────────────────────
            DaemonRequest::UpdateServerConfig {
                base_url,
                username,
                password,
            } => {
                self.core
                    .update_server_config(&base_url, &username, &password)
                    .await
                    .map_err(err)?;
                Ok(DaemonResponse::Ok)
            }
            DaemonRequest::TestServerConnection {
                base_url,
                username,
                password,
            } => {
                let (ok, message) = self
                    .core
                    .test_server_connection(&base_url, &username, &password)
                    .await;
                Ok(DaemonResponse::ConnectionTestResult { ok, message })
            }
            DaemonRequest::SetTheme(name) => {
                self.core.set_theme(&name).await.map_err(err)?;
                Ok(DaemonResponse::Ok)
            }
            DaemonRequest::SetCavaEnabled(on) => {
                self.core.set_cava_enabled(on).await.map_err(err)?;
                Ok(DaemonResponse::Ok)
            }
            DaemonRequest::SetCavaSize(sz) => {
                self.core.set_cava_size(sz).await.map_err(err)?;
                Ok(DaemonResponse::Ok)
            }
            DaemonRequest::SetDaemonEnabled(on) => {
                self.core.set_daemon_enabled(on).await.map_err(err)?;
                Ok(DaemonResponse::Ok)
            }

            // ── Lifecycle ───────────────────────────────────────────
            DaemonRequest::Subscribe => {
                // Subscription is via the separate `subscribe()` trait
                // method, not the request channel. A spurious
                // `Subscribe` request here is a no-op for back-compat.
                warn!("Subscribe sent as request; use DaemonClient::subscribe instead");
                Ok(DaemonResponse::Ok)
            }
            DaemonRequest::Snapshot => {
                let snap = self.core.snapshot().await;
                Ok(DaemonResponse::Snapshot(Box::new(snap)))
            }
            DaemonRequest::Shutdown => {
                self.core.quit_mpv().await;
                Ok(DaemonResponse::Ok)
            }
            DaemonRequest::Ping => Ok(DaemonResponse::Pong),
        }
    }

    fn subscribe(&self) -> broadcast::Receiver<DaemonEvent> {
        self.core.subscribe()
    }
}

impl InProcessClient {
    async fn enqueue_songs(
        &self,
        songs: Vec<crate::subsonic::models::Child>,
        mode: EnqueueMode,
    ) -> Result<DaemonResponse, IpcError> {
        match mode {
            EnqueueMode::Replace { play_from } => {
                {
                    let mut state = self.core.state.write().await;
                    state.daemon.queue = songs;
                    state.daemon.queue_position = None;
                }
                self.core.broadcast_queue_changed().await;
                if let Some(idx) = play_from {
                    self.core.play_queue_position(idx).await.map_err(err)?;
                }
            }
            EnqueueMode::Append => {
                {
                    let mut state = self.core.state.write().await;
                    state.daemon.queue.extend(songs);
                }
                self.core.broadcast_queue_changed().await;
            }
            EnqueueMode::InsertAfter(pos) => {
                {
                    let mut state = self.core.state.write().await;
                    let insert_at = (pos + 1).min(state.daemon.queue.len());
                    for (i, song) in songs.into_iter().enumerate() {
                        state.daemon.queue.insert(insert_at + i, song);
                    }
                }
                self.core.broadcast_queue_changed().await;
            }
        }
        Ok(DaemonResponse::Ok)
    }
}

/// Convert a domain error into an `IpcError`. In-process the original
/// `crate::error::Error` collapses into a string; phase 4's socket path
/// will preserve more structure on the wire.
fn err(e: crate::error::Error) -> IpcError {
    IpcError::Daemon(e.to_string())
}
