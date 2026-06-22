//! TUI-side event pump: subscribes to daemon broadcast and mirrors events into local state.

use std::sync::Arc;

use tokio::sync::broadcast;
use tracing::{info, warn};

use crate::app::state::{SharedClientState, SharedDaemonState};
use crate::ipc::{DaemonClient, DaemonEvent, DaemonRequest, DaemonResponse};
use crate::ui::cover_art::CoverArtState;

pub(crate) async fn run_event_pump(
    client: Arc<dyn DaemonClient>,
    daemon_state: SharedDaemonState,
    client_state: SharedClientState,
    cover_art: Arc<std::sync::Mutex<CoverArtState>>,
    mut rx: broadcast::Receiver<DaemonEvent>,
) {
    loop {
        match rx.recv().await {
            Ok(ev) => apply_event(&daemon_state, &client_state, &client, &cover_art, ev).await,
            Err(broadcast::error::RecvError::Lagged(n)) => {
                warn!("Event pump lagged by {}; resnapshot + resubscribe", n);
                let new_rx = client.subscribe();
                if let Ok(DaemonResponse::Snapshot(snap)) =
                    client.request(DaemonRequest::Snapshot).await
                {
                    let mut ds = daemon_state.write().await;
                    *ds = *snap;
                }
                rx = new_rx;
            }
            Err(broadcast::error::RecvError::Closed) => {
                warn!("Daemon event broadcast closed; pump exiting");
                break;
            }
        }
    }
}

/// Lock order: daemon, then client. Same everywhere - avoids deadlock.
// significant_drop_tightening: tokio guard held to scope; not tightened (early-drop is borrow-blocked, spans a trailing await, or saves nothing before return).
#[allow(clippy::significant_drop_tightening)]
pub async fn apply_event(
    daemon_state: &SharedDaemonState,
    client_state: &SharedClientState,
    client: &Arc<dyn DaemonClient>,
    cover_art: &Arc<std::sync::Mutex<CoverArtState>>,
    ev: DaemonEvent,
) {
    match ev {
        DaemonEvent::QueueChanged { queue, position } => {
            let mut ds = daemon_state.write().await;
            ds.queue = queue;
            ds.queue_position = position;
        }
        DaemonEvent::NowPlayingChanged(np) => {
            apply_now_playing_changed(daemon_state, client, cover_art, *np).await;
        }
        DaemonEvent::PositionTick(pos) => {
            let mut ds = daemon_state.write().await;
            ds.now_playing.position = pos;
        }
        DaemonEvent::StarredChanged(songs) => {
            let mut ds = daemon_state.write().await;
            ds.library.starred_songs = songs;
            ds.library.rebuild_starred_index();
        }
        DaemonEvent::SongStarChanged { id, starred } => {
            apply_song_star_changed(daemon_state, client_state, id, starred).await;
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
            let lib = &mut ds.library;
            crate::daemon::library::cache_insert(
                &mut lib.albums_cache,
                &mut lib.albums_cache_order,
                artist_id,
                albums,
                crate::daemon::library::ALBUMS_CACHE_CAP,
            );
        }
        DaemonEvent::AlbumSongsChanged { album_id, songs } => {
            let mut ds = daemon_state.write().await;
            let lib = &mut ds.library;
            crate::daemon::library::cache_insert(
                &mut lib.album_songs_cache,
                &mut lib.album_songs_cache_order,
                album_id,
                songs,
                crate::daemon::library::ALBUM_SONGS_CACHE_CAP,
            );
        }
        DaemonEvent::PlaylistsChanged(playlists) => {
            let mut ds = daemon_state.write().await;
            ds.library.playlists = playlists;
        }
        DaemonEvent::MusicFoldersChanged(folders) => {
            let mut ds = daemon_state.write().await;
            ds.library.music_folders = folders;
        }
        DaemonEvent::PlaylistSongsChanged { playlist_id, songs } => {
            let mut ds = daemon_state.write().await;
            let lib = &mut ds.library;
            crate::daemon::library::cache_insert(
                &mut lib.playlist_songs_cache,
                &mut lib.playlist_songs_cache_order,
                playlist_id,
                songs,
                crate::daemon::library::PLAYLIST_SONGS_CACHE_CAP,
            );
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
            apply_config_changed(daemon_state, client_state, client, cover_art, cfg).await;
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
            cs.notify_error("Daemon shut down, disconnecting");
            cs.should_quit = true;
        }
        DaemonEvent::LibraryVersionChanged(_) => {}
    }
}

/// Apply `NowPlayingChanged`: store the new now-playing and refresh cover art.
async fn apply_now_playing_changed(
    daemon_state: &SharedDaemonState,
    client: &Arc<dyn DaemonClient>,
    cover_art: &Arc<std::sync::Mutex<CoverArtState>>,
    np: crate::daemon::state::NowPlaying,
) {
    let new_cover_id = np
        .song
        .as_ref()
        .and_then(crate::subsonic::models::Child::cover_id);
    let cover_art_enabled = {
        let mut ds = daemon_state.write().await;
        let enabled = ds.config.cover_art;
        ds.now_playing = np;
        enabled
    };
    if cover_art_enabled {
        if let Some(id) = new_cover_id {
            let should_fetch = {
                let mut guard = cover_art
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                if guard.current_id.as_deref() == Some(id.as_str()) {
                    false
                } else {
                    guard.set_pending(id.clone());
                    true
                }
            };
            if should_fetch {
                info!("Fetching cover art id={}", id);
                match client
                    .request(DaemonRequest::FetchCoverArt {
                        id: id.clone(),
                        size: 512,
                    })
                    .await
                {
                    Ok(DaemonResponse::CoverArt(bytes)) => {
                        info!("Cover art bytes received: {} bytes", bytes.len());
                        if !bytes.is_empty() {
                            let mut guard = cover_art
                                .lock()
                                .unwrap_or_else(std::sync::PoisonError::into_inner);
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
            let mut guard = cover_art
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            guard.clear();
        }
    }
}

/// Apply `ConfigChanged`: mirror config into daemon + client state, then
/// refresh cover art for the new cover-art setting.
async fn apply_config_changed(
    daemon_state: &SharedDaemonState,
    client_state: &SharedClientState,
    client: &Arc<dyn DaemonClient>,
    cover_art: &Arc<std::sync::Mutex<CoverArtState>>,
    cfg: crate::config::Config,
) {
    let repeat_mode = cfg.repeat_mode;
    let cover_art_enabled = cfg.cover_art;
    let cover_art_size = cfg.cover_art_size;
    let auto_continue = cfg.auto_continue;
    let scrobble = cfg.scrobble;
    let notifications = cfg.notifications;
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
        cs.settings_state.scrobble = scrobble;
        cs.settings_state.notifications = notifications;
    }

    if cover_art_enabled {
        let current_id = {
            let ds = daemon_state.read().await;
            ds.now_playing
                .song
                .as_ref()
                .and_then(crate::subsonic::models::Child::cover_id)
        };
        if let Some(id) = current_id {
            let should_fetch = {
                let mut guard = cover_art
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                if guard.current_id.as_deref() == Some(id.as_str()) {
                    false
                } else {
                    guard.set_pending(id.clone());
                    true
                }
            };
            if should_fetch {
                info!("Cover art enabled; fetching current id={}", id);
                if let Ok(DaemonResponse::CoverArt(bytes)) = client
                    .request(DaemonRequest::FetchCoverArt {
                        id: id.clone(),
                        size: 512,
                    })
                    .await
                {
                    if !bytes.is_empty() {
                        let mut guard = cover_art
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner);
                        guard.load(id, &bytes);
                    }
                }
            }
        }
    } else {
        let mut guard = cover_art
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        guard.clear();
    }
}

/// Apply `SongStarChanged`: flip the star marker across every cached copy of
/// the song in daemon + client state.
async fn apply_song_star_changed(
    daemon_state: &SharedDaemonState,
    client_state: &SharedClientState,
    id: String,
    starred: bool,
) {
    let marker = if starred { Some("1".to_string()) } else { None };
    let update = |song: &mut crate::subsonic::models::Child| {
        if song.id == id {
            song.starred.clone_from(&marker);
        }
    };
    {
        let mut ds = daemon_state.write().await;
        for song in &mut ds.queue {
            update(song);
        }
        for song in &mut ds.library.random_songs {
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
                np.starred.clone_from(&marker);
            }
        }
        if starred {
            ds.library.starred_ids.insert(id.clone());
        } else {
            ds.library.starred_ids.remove(&id);
        }
    }
    {
        let mut cs = client_state.write().await;
        for song in &mut cs.artists.songs {
            update(song);
        }
        for song in &mut cs.playlists.songs {
            update(song);
        }
    }
}
