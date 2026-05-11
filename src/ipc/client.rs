//! `DaemonClient` trait + `InProcessClient` dispatch.

#![allow(dead_code)]

use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::broadcast;
use tracing::warn;

use crate::daemon::DaemonCore;
use crate::ipc::protocol::{
    DaemonEvent, DaemonRequest, DaemonResponse, EnqueueMode, IpcError,
};

/// TUI's view of the daemon: every command via `request`, every state
/// subscription via `subscribe`.
#[async_trait]
pub trait DaemonClient: Send + Sync {
    async fn request(&self, req: DaemonRequest) -> Result<DaemonResponse, IpcError>;
    /// Slow consumers may see `RecvError::Lagged`; resubscribe in that case.
    fn subscribe(&self) -> broadcast::Receiver<DaemonEvent>;
}

pub struct InProcessClient {
    core: Arc<DaemonCore>,
}

impl InProcessClient {
    pub fn new(core: Arc<DaemonCore>) -> Self {
        Self { core }
    }

    pub fn core(&self) -> &Arc<DaemonCore> {
        &self.core
    }
}

#[async_trait]
impl DaemonClient for InProcessClient {
    async fn request(&self, req: DaemonRequest) -> Result<DaemonResponse, IpcError> {
        match req {
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

            DaemonRequest::EnqueueSongs { songs, mode } => {
                self.enqueue_songs(songs, mode).await
            }
            DaemonRequest::PlayQueueIndex(pos) => {
                self.core.play_queue_position(pos).await.map_err(err)?;
                Ok(DaemonResponse::Ok)
            }
            DaemonRequest::RemoveFromQueue(pos) => {
                let was_playing;
                let new_len;
                {
                    let mut state = self.core.state.write().await;
                    if pos >= state.queue.len() {
                        return Ok(DaemonResponse::Ok);
                    }
                    was_playing = state.queue_position == Some(pos);
                    state.queue.remove(pos);
                    new_len = state.queue.len();
                    if let Some(cur) = state.queue_position {
                        if pos < cur {
                            state.queue_position = Some(cur - 1);
                        } else if pos == cur {
                            state.queue_position = None;
                        }
                    }
                }
                if was_playing {
                    if pos < new_len {
                        self.core.play_queue_position(pos).await.map_err(err)?;
                    } else {
                        self.core.halt_keep_queue().await;
                        self.core.broadcast_queue_changed().await;
                    }
                } else {
                    self.core.broadcast_queue_changed().await;
                }
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
            DaemonRequest::ShuffleLibrary => {
                self.core.shuffle_library().await.map_err(err)?;
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
            DaemonRequest::ToggleStarSong(id) => {
                self.core.toggle_star_song(&id).await.map_err(err)?;
                Ok(DaemonResponse::Ok)
            }
            DaemonRequest::LoadArtist(id) => {
                self.core.load_artist(&id).await;
                let state = self.core.state.read().await;
                let albums = state
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
            DaemonRequest::Search {
                query,
                artist_count,
                album_count,
                song_count,
            } => {
                let results = self
                    .core
                    .search(&query, artist_count, album_count, song_count)
                    .await;
                Ok(DaemonResponse::SearchResults(results))
            }

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
            DaemonRequest::SetAutoContinue(on) => {
                self.core.set_auto_continue(on).await.map_err(err)?;
                Ok(DaemonResponse::Ok)
            }
            DaemonRequest::SetRepeatMode(mode) => {
                self.core.set_repeat_mode(mode).await.map_err(err)?;
                Ok(DaemonResponse::Ok)
            }
            DaemonRequest::SetCoverArtEnabled(on) => {
                self.core.set_cover_art_enabled(on).await.map_err(err)?;
                Ok(DaemonResponse::Ok)
            }
            DaemonRequest::FetchCoverArt { id, size } => {
                let bytes = self.core.get_cover_art(&id, size).await;
                Ok(DaemonResponse::CoverArt(bytes))
            }

            DaemonRequest::Subscribe => {
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
                    state.queue = songs;
                    state.queue_position = None;
                }
                self.core.broadcast_queue_changed().await;
                if let Some(idx) = play_from {
                    self.core.play_queue_position(idx).await.map_err(err)?;
                }
            }
            EnqueueMode::Append => {
                {
                    let mut state = self.core.state.write().await;
                    state.queue.extend(songs);
                }
                self.core.broadcast_queue_changed().await;
            }
            EnqueueMode::InsertAfter(pos) => {
                {
                    let mut state = self.core.state.write().await;
                    let insert_at = (pos + 1).min(state.queue.len());
                    for (i, song) in songs.into_iter().enumerate() {
                        state.queue.insert(insert_at + i, song);
                    }
                }
                self.core.broadcast_queue_changed().await;
            }
        }
        Ok(DaemonResponse::Ok)
    }
}

fn err(e: crate::error::Error) -> IpcError {
    IpcError::Daemon(e.to_string())
}
