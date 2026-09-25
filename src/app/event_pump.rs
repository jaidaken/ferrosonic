//! TUI-side event pump: subscribes to daemon broadcast and mirrors events into local state.

use std::sync::Arc;

use tokio::sync::broadcast;
use tracing::{info, warn};

use crate::app::input_library::sort_albums;
use crate::app::page_state::LibraryView;
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
                // A missed LibraryInvalidated leaves expanded artists without albums.
                apply_library_invalidated(&daemon_state, &client_state, &client).await;
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
    let Some(ev) = apply_library_event(daemon_state, ev).await else {
        return;
    };
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
        DaemonEvent::StreamStatsChanged {
            codec,
            bitrate_kbps,
            download_bps,
        } => {
            let mut ds = daemon_state.write().await;
            ds.now_playing.codec = codec;
            ds.now_playing.bitrate_kbps = bitrate_kbps;
            ds.now_playing.download_bps = download_bps;
        }
        DaemonEvent::VolumeChanged(vol) => {
            {
                let mut ds = daemon_state.write().await;
                ds.config.volume = vol;
            }
            let mut cs = client_state.write().await;
            cs.volume_adjusted_at = Some(std::time::Instant::now());
        }
        DaemonEvent::SongStarChanged { id, starred } => {
            apply_song_star_changed(daemon_state, client_state, id, starred).await;
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
        DaemonEvent::LibraryInvalidated => {
            apply_library_invalidated(daemon_state, client_state, client).await;
        }
        DaemonEvent::RepeatModeChanged(mode) => {
            {
                let mut ds = daemon_state.write().await;
                ds.config.repeat_mode = mode;
            }
            let mut cs = client_state.write().await;
            cs.settings_state.repeat_mode = mode;
        }
        // Already mirrored by `apply_library_event`; listed so the match stays
        // exhaustive when a new event variant is added.
        DaemonEvent::StarredChanged(_)
        | DaemonEvent::RandomChanged(_)
        | DaemonEvent::RadioStationsChanged(_)
        | DaemonEvent::ArtistsChanged(_)
        | DaemonEvent::AlbumsChanged { .. }
        | DaemonEvent::AlbumSongsChanged { .. }
        | DaemonEvent::PlaylistsChanged(_)
        | DaemonEvent::MusicFoldersChanged(_)
        | DaemonEvent::PlaylistSongsChanged { .. }
        // The pull-style library-version signal is unused by this TUI.
        | DaemonEvent::LibraryVersionChanged(_) => {}
        DaemonEvent::Shutdown => {
            let mut cs = client_state.write().await;
            cs.notify_error("Daemon shut down, disconnecting");
            cs.should_quit = true;
        }
    }
}

/// Mirror a library-cache event into `daemon_state`. Returns `None` when the
/// event was one of these pure list/cache updates (fully handled), or gives
/// the event back for `apply_event`'s interactive arms.
// significant_drop_tightening: tokio guard held to scope; not tightened (early-drop is borrow-blocked, spans a trailing await, or saves nothing before return).
#[allow(clippy::significant_drop_tightening)]
async fn apply_library_event(
    daemon_state: &SharedDaemonState,
    ev: DaemonEvent,
) -> Option<DaemonEvent> {
    match ev {
        DaemonEvent::StarredChanged(songs) => {
            let mut ds = daemon_state.write().await;
            ds.library.starred_songs = songs;
            ds.library.rebuild_starred_index();
        }
        DaemonEvent::RandomChanged(songs) => {
            daemon_state.write().await.library.random_songs = songs;
        }
        DaemonEvent::RadioStationsChanged(stations) => {
            daemon_state.write().await.library.radio_stations = stations;
        }
        DaemonEvent::ArtistsChanged(artists) => {
            daemon_state.write().await.library.artists = artists;
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
            daemon_state.write().await.library.playlists = playlists;
        }
        DaemonEvent::MusicFoldersChanged(folders) => {
            daemon_state.write().await.library.music_folders = folders;
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
        other => return Some(other),
    }
    None
}

/// Apply `LibraryInvalidated`: drop the mirrored album and track caches, then
/// reload the albums of expanded artists and an open flat album list.
/// Takes the daemon and client locks one after the other, never together.
pub(crate) async fn apply_library_invalidated(
    daemon_state: &SharedDaemonState,
    client_state: &SharedClientState,
    client: &Arc<dyn DaemonClient>,
) {
    {
        let mut ds = daemon_state.write().await;
        let lib = &mut ds.library;
        lib.albums_cache.clear();
        lib.albums_cache_order.clear();
        lib.album_songs_cache.clear();
        lib.album_songs_cache_order.clear();
        lib.playlist_songs_cache.clear();
        lib.playlist_songs_cache_order.clear();
        lib.all_albums.clear();
        drop(ds);
    }
    let (expanded, reload_album_list) = {
        let mut cs = client_state.write().await;
        let reload = cs.artists.view == LibraryView::AlbumList;
        if !reload {
            // Refetched on the next switch to the album view.
            cs.artists.albums.clear();
        }
        let expanded: Vec<String> = cs.artists.expanded.iter().cloned().collect();
        (expanded, reload)
    };
    let reload = async {
        reload_expanded_artists(daemon_state, client, expanded).await;
        if reload_album_list {
            reload_album_list_view(client_state, client).await;
        }
    };
    if tokio::time::timeout(LIBRARY_RELOAD_TIMEOUT, reload)
        .await
        .is_err()
    {
        warn!(
            "Library reload after a reset exceeded {}s; the rest loads on demand",
            LIBRARY_RELOAD_TIMEOUT.as_secs()
        );
    }
}

/// Upper bound on the post-reset reload, so a stalled server cannot hold the event pump.
const LIBRARY_RELOAD_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);

/// Artist album fetches in flight at once during the post-reset reload.
const ARTIST_RELOAD_CONCURRENCY: usize = 4;

/// Refetch the albums of each expanded artist, a few at a time. A failed
/// fetch stays uncached so the next expand retries it.
async fn reload_expanded_artists(
    daemon_state: &SharedDaemonState,
    client: &Arc<dyn DaemonClient>,
    expanded: Vec<String>,
) {
    use futures::StreamExt;
    let mut replies = futures::stream::iter(expanded)
        .map(|artist_id| async move {
            let reply = client
                .request(DaemonRequest::LoadArtist(artist_id.clone()))
                .await;
            (artist_id, reply)
        })
        .buffer_unordered(ARTIST_RELOAD_CONCURRENCY);
    while let Some((artist_id, reply)) = replies.next().await {
        match reply {
            Ok(DaemonResponse::ArtistAlbums(albums)) => {
                let mut ds = daemon_state.write().await;
                let lib = &mut ds.library;
                crate::daemon::library::cache_insert(
                    &mut lib.albums_cache,
                    &mut lib.albums_cache_order,
                    artist_id,
                    albums,
                    crate::daemon::library::ALBUMS_CACHE_CAP,
                );
                drop(ds);
            }
            Ok(other) => warn!("LoadArtist after a library reset: unexpected {:?}", other),
            Err(e) => warn!("LoadArtist {artist_id} after a library reset failed: {e}"),
        }
    }
}

/// Refetch the flat album list and sort it by the order chosen at store time,
/// so a sort change made during the fetch applies to the new list.
async fn reload_album_list_view(client_state: &SharedClientState, client: &Arc<dyn DaemonClient>) {
    match client.request(DaemonRequest::LoadAllAlbums).await {
        Ok(DaemonResponse::AllAlbums(mut albums)) => {
            let mut cs = client_state.write().await;
            sort_albums(&mut albums, cs.artists.album_sort);
            let selected_id = cs
                .artists
                .album_selected
                .and_then(|i| cs.artists.albums.get(i))
                .map(|a| a.id.clone());
            cs.artists.album_selected = selected_id
                .and_then(|id| albums.iter().position(|a| a.id == id))
                .or_else(|| (!albums.is_empty()).then_some(0));
            cs.artists.album_scroll_offset = cs
                .artists
                .album_scroll_offset
                .min(albums.len().saturating_sub(1));
            cs.artists.albums = albums;
        }
        Ok(other) => warn!(
            "LoadAllAlbums after a library reset: unexpected {:?}",
            other
        ),
        Err(e) => warn!("LoadAllAlbums after a library reset failed: {}", e),
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
                let rows = daemon_state.read().await.config.cover_art_size;
                let size = {
                    let guard = cover_art
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    crate::ui::cover_art::cover_fetch_size(guard.cell_size.1, rows)
                };
                match client
                    .request(DaemonRequest::FetchCoverArt {
                        id: id.clone(),
                        size,
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
                let size = {
                    let guard = cover_art
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    crate::ui::cover_art::cover_fetch_size(guard.cell_size.1, cover_art_size)
                };
                if let Ok(DaemonResponse::CoverArt(bytes)) = client
                    .request(DaemonRequest::FetchCoverArt {
                        id: id.clone(),
                        size,
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
