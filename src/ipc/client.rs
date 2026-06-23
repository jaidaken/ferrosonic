//! `DaemonClient` trait + `InProcessClient` dispatch.

use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::broadcast;
use tracing::warn;

use crate::daemon::DaemonCore;
use crate::ipc::protocol::{DaemonEvent, DaemonRequest, DaemonResponse, EnqueueMode, IpcError};

/// TUI's view of the daemon: every command via `request`, every state
/// subscription via `subscribe`.
#[async_trait]
pub trait DaemonClient: Send + Sync {
    /// Send one command and await its reply.
    async fn request(&self, req: DaemonRequest) -> Result<DaemonResponse, IpcError>;
    /// Slow consumers may see `RecvError::Lagged`; resubscribe in that case.
    fn subscribe(&self) -> broadcast::Receiver<DaemonEvent>;
}

/// `DaemonClient` that calls a same-process `DaemonCore` directly (standalone mode).
pub struct InProcessClient {
    core: Arc<DaemonCore>,
}

impl InProcessClient {
    /// Wrap an existing core.
    pub const fn new(core: Arc<DaemonCore>) -> Self {
        Self { core }
    }

    /// Borrow the wrapped core.
    #[must_use]
    pub const fn core(&self) -> &Arc<DaemonCore> {
        &self.core
    }
}

#[async_trait]
impl DaemonClient for InProcessClient {
    // Exhaustive ~50-command router; one flat match compiles to a single jump table and reads clearer than nesting the wire-protocol enum.
    #[allow(clippy::too_many_lines)]
    async fn request(&self, req: DaemonRequest) -> Result<DaemonResponse, IpcError> {
        let core = &self.core;
        match req {
            DaemonRequest::Pause => ok_response(core.pause_playback().await),
            DaemonRequest::Resume => ok_response(core.resume_playback().await),
            DaemonRequest::TogglePause => ok_response(core.toggle_pause().await),
            DaemonRequest::Stop => ok_response(core.stop_keep_queue().await),
            DaemonRequest::Seek(pos) => ok_response(core.seek(pos).await),
            DaemonRequest::SeekRelative(off) => ok_response(core.seek_relative(off).await),
            DaemonRequest::Next => ok_response(core.next_track().await),
            DaemonRequest::Previous => ok_response(core.prev_track().await),
            DaemonRequest::SetVolume(v) => ok_response(core.set_volume(v).await),
            DaemonRequest::EnqueueSongs { songs, mode } => self.enqueue_songs(songs, mode).await,
            DaemonRequest::PlayQueueIndex(pos) => ok_response(
                core.play_queue_position(pos, crate::daemon::core::PlayMode::Direct)
                    .await,
            ),
            DaemonRequest::RemoveFromQueue(pos) => self.handle_remove_from_queue(pos).await,
            DaemonRequest::ClearQueue => ok_response(core.stop_playback().await),
            DaemonRequest::ShuffleQueue => {
                core.shuffle_queue().await;
                Ok(DaemonResponse::Ok)
            }
            DaemonRequest::ShuffleLibrary => ok_response(core.shuffle_library().await),
            DaemonRequest::MoveQueueItem { from, to } => {
                core.move_queue_item(from, to).await;
                Ok(DaemonResponse::Ok)
            }
            DaemonRequest::ClearQueueHistory => Ok(DaemonResponse::HistoryCleared(
                core.clear_queue_history().await,
            )),
            DaemonRequest::RefreshStarred => {
                core.refresh_starred().await;
                Ok(DaemonResponse::Ok)
            }
            DaemonRequest::RefreshRandom => {
                core.refresh_random().await;
                Ok(DaemonResponse::Ok)
            }
            DaemonRequest::RefreshArtists => {
                core.refresh_artists().await;
                core.refresh_music_folders().await;
                Ok(DaemonResponse::Ok)
            }
            DaemonRequest::SetMusicFolder(id) => ok_response(core.set_music_folder(id).await),
            DaemonRequest::RefreshPlaylists => {
                core.refresh_playlists().await;
                Ok(DaemonResponse::Ok)
            }
            DaemonRequest::CreatePlaylist { name, song_ids } => {
                ok_response(core.create_playlist(&name, &song_ids).await)
            }
            DaemonRequest::RenamePlaylist { id, name } => {
                ok_response(core.rename_playlist(&id, &name).await)
            }
            DaemonRequest::DeletePlaylist { id } => ok_response(core.delete_playlist(&id).await),
            DaemonRequest::AddSongToPlaylist {
                playlist_id,
                song_id,
            } => Ok(DaemonResponse::PlaylistSongs(
                core.playlist_add_song(&playlist_id, &song_id)
                    .await
                    .map_err(err)?,
            )),
            DaemonRequest::RemovePlaylistSong { playlist_id, index } => {
                Ok(DaemonResponse::PlaylistSongs(
                    core.playlist_remove_song(&playlist_id, index)
                        .await
                        .map_err(err)?,
                ))
            }
            DaemonRequest::ReorderPlaylist {
                playlist_id,
                song_ids,
            } => Ok(DaemonResponse::PlaylistSongs(
                core.playlist_reorder(&playlist_id, &song_ids)
                    .await
                    .map_err(err)?,
            )),
            DaemonRequest::ToggleStarSong(id) => ok_response(core.toggle_star_song(&id).await),
            DaemonRequest::LoadArtist(id) => self.handle_load_artist(&id).await,
            DaemonRequest::LoadAllAlbums => {
                Ok(DaemonResponse::AllAlbums(core.load_all_albums().await))
            }
            DaemonRequest::LoadAlbum(id) => {
                Ok(DaemonResponse::AlbumSongs(core.load_album_songs(&id).await))
            }
            DaemonRequest::LoadPlaylist(id) => Ok(DaemonResponse::PlaylistSongs(
                core.load_playlist_songs(&id).await,
            )),
            DaemonRequest::Search {
                query,
                artist_count,
                album_count,
                song_count,
            } => Ok(DaemonResponse::SearchResults(
                core.search(&query, artist_count, album_count, song_count)
                    .await,
            )),

            DaemonRequest::UpdateServerConfig {
                base_url,
                username,
                password,
            } => Ok(DaemonResponse::ServerConfigSaved(
                core.update_server_config(&base_url, &username, &password)
                    .await
                    .map_err(err)?,
            )),
            DaemonRequest::TestServerConnection {
                base_url,
                username,
                password,
            } => {
                let (ok, message) = core
                    .test_server_connection(&base_url, &username, &password)
                    .await;
                Ok(DaemonResponse::ConnectionTestResult { ok, message })
            }
            DaemonRequest::SetTheme(name) => ok_response(core.set_theme(&name).await),
            DaemonRequest::SetCavaEnabled(on) => ok_response(core.set_cava_enabled(on).await),
            DaemonRequest::SetCavaSize(sz) => ok_response(core.set_cava_size(sz).await),
            DaemonRequest::SetDaemonEnabled(on) => ok_response(core.set_daemon_enabled(on).await),
            DaemonRequest::SetAutoContinue(on) => ok_response(core.set_auto_continue(on).await),
            DaemonRequest::SetScrobble(on) => ok_response(core.set_scrobble(on).await),
            DaemonRequest::SetNotifications(on) => ok_response(core.set_notifications(on).await),
            DaemonRequest::SetRepeatMode(mode) => ok_response(core.set_repeat_mode(mode).await),
            DaemonRequest::SetCoverArtEnabled(on) => {
                ok_response(core.set_cover_art_enabled(on).await)
            }
            DaemonRequest::SetCoverArtSize(sz) => ok_response(core.set_cover_art_size(sz).await),
            DaemonRequest::FetchCoverArt { id, size } => {
                self.handle_fetch_cover_art(&id, size).await
            }
            DaemonRequest::Subscribe => {
                warn!("Subscribe sent as request; use DaemonClient::subscribe instead");
                Ok(DaemonResponse::Ok)
            }
            DaemonRequest::Snapshot => {
                Ok(DaemonResponse::Snapshot(Box::new(core.snapshot().await)))
            }
            DaemonRequest::Shutdown => self.handle_shutdown().await,
            DaemonRequest::Ping => Ok(DaemonResponse::Pong),
        }
    }

    fn subscribe(&self) -> broadcast::Receiver<DaemonEvent> {
        self.core.subscribe()
    }
}

impl InProcessClient {
    // significant_drop_tightening: tokio guard held to scope; not tightened (early-drop is borrow-blocked, spans a trailing await, or saves nothing before return).
    #[allow(clippy::significant_drop_tightening)]
    async fn enqueue_songs(
        &self,
        songs: Vec<crate::subsonic::models::Child>,
        mode: EnqueueMode,
    ) -> Result<DaemonResponse, IpcError> {
        match mode {
            EnqueueMode::Replace { play_from } => {
                self.core
                    .replace_queue_and_play(
                        songs,
                        play_from,
                        crate::daemon::core::PlayMode::Buffered,
                    )
                    .await
                    .map_err(err)?;
            }
            EnqueueMode::Append => {
                let resync = {
                    let mut state = self.core.state.write().await;
                    let old_len = state.queue.len();
                    state.queue.extend(songs);
                    // The appended block becomes the next track only when the
                    // current track was the last entry.
                    matches!(state.queue_position, Some(cur) if cur + 1 == old_len)
                };
                self.core.broadcast_queue_changed().await;
                if resync {
                    self.core.resync_gapless_preload().await;
                }
            }
            EnqueueMode::InsertAfter(pos) => {
                // The Some arm updates queue_position in place; if-let reads clearer than map_or_else.
                #[allow(clippy::option_if_let_else)]
                let resync = {
                    let mut state = self.core.state.write().await;
                    let insert_at = (pos + 1).min(state.queue.len());
                    let n = songs.len();
                    for (i, song) in songs.into_iter().enumerate() {
                        state.queue.insert(insert_at + i, song);
                    }
                    // Keep the now-playing pointer on the same song; the gapless
                    // preload is stale only when we insert into the next slot.
                    match state.queue_position {
                        Some(cur) => {
                            if insert_at <= cur {
                                state.queue_position = Some(cur + n);
                            }
                            insert_at == cur + 1
                        }
                        None => false,
                    }
                };
                self.core.broadcast_queue_changed().await;
                if resync {
                    self.core.resync_gapless_preload().await;
                }
            }
        }
        Ok(DaemonResponse::Ok)
    }

    async fn handle_remove_from_queue(&self, pos: usize) -> Result<DaemonResponse, IpcError> {
        // State.write block sets the queue_position+state.Stopped sentinel before mpv touches, so position-tick poll sees state=Stopped and bails; lock order stays state-then-mpv with no overlap.
        let was_playing;
        let new_len;
        let must_stop;
        let removed_next_up;
        {
            let mut state = self.core.state.write().await;
            if pos >= state.queue.len() {
                return Ok(DaemonResponse::Ok);
            }
            was_playing = state.queue_position == Some(pos);
            // The gapless preload is stale only when the removed entry
            // was the next-up track (one past the current position).
            removed_next_up = pos > 0 && state.queue_position == Some(pos - 1);
            state.queue.remove(pos);
            new_len = state.queue.len();
            if let Some(cur) = state.queue_position {
                if pos < cur {
                    state.queue_position = Some(cur - 1);
                } else if pos == cur {
                    state.queue_position = None;
                }
            }
            must_stop = was_playing && pos >= new_len;
            if must_stop {
                state.now_playing.state = crate::daemon::state::PlaybackState::Stopped;
                state.now_playing.song = None;
                state.now_playing.position = 0.0;
                state.now_playing.duration = 0.0;
                state.now_playing.sample_rate = None;
                state.now_playing.bit_depth = None;
                state.now_playing.format = None;
                state.now_playing.channels = None;
            }
        }
        if must_stop {
            let mut mpv = self.core.mpv.lock().await;
            if let Err(e) = mpv.stop().await {
                tracing::error!("Failed to stop on remove: {}", e);
            }
        }
        if was_playing && !must_stop {
            self.core
                .play_queue_position(pos, crate::daemon::core::PlayMode::Direct)
                .await
                .map_err(err)?;
        } else if must_stop {
            self.core.broadcast_now_playing().await;
            self.core.broadcast_queue_changed().await;
        } else {
            self.core.broadcast_queue_changed().await;
            if removed_next_up {
                self.core.resync_gapless_preload().await;
            }
        }
        Ok(DaemonResponse::Ok)
    }

    async fn handle_load_artist(&self, id: &str) -> Result<DaemonResponse, IpcError> {
        self.core.load_artist(id).await;
        let albums = {
            let state = self.core.state.read().await;
            state
                .library
                .albums_cache
                .get(id)
                .cloned()
                .unwrap_or_default()
        };
        Ok(DaemonResponse::ArtistAlbums(albums))
    }

    async fn handle_fetch_cover_art(
        &self,
        id: &str,
        size: u32,
    ) -> Result<DaemonResponse, IpcError> {
        const MAX_SIZE: u32 = 2048;
        const MAX_ID_LEN: usize = 256;
        if id.len() > MAX_ID_LEN
            || id
                .chars()
                .any(|c| matches!(c, '/' | '?' | '#' | '\\') || c.is_control())
        {
            return Ok(DaemonResponse::CoverArt(Vec::new()));
        }
        let size = size.clamp(1, MAX_SIZE);
        let bytes = self.core.get_cover_art(id, size).await;
        Ok(DaemonResponse::CoverArt(bytes))
    }

    async fn handle_shutdown(&self) -> Result<DaemonResponse, IpcError> {
        let _ = self.core.event_tx.send(crate::ipc::DaemonEvent::Shutdown);
        let _ = tokio::time::timeout(std::time::Duration::from_secs(3), self.core.quit_mpv()).await;
        // Stop the IPC accept loop so the daemon process actually exits;
        // without this it broadcasts Shutdown but keeps listening.
        self.core.request_shutdown();
        Ok(DaemonResponse::Ok)
    }
}

// Collapse the common "run a fallible core call, reply Ok" dispatch arm. The
// success value is discarded (these requests reply with a plain Ok).
fn ok_response<T>(r: Result<T, crate::error::Error>) -> Result<DaemonResponse, IpcError> {
    r.map_err(err)?;
    Ok(DaemonResponse::Ok)
}

// Used as a map_err fn-item; map_err passes the owned error by value.
#[allow(clippy::needless_pass_by_value)]
fn err(e: crate::error::Error) -> IpcError {
    IpcError::Daemon(e.to_string())
}
