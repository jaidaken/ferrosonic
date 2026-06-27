//! MPRIS2 D-Bus server.

use std::sync::Arc;

use mpris_server::{
    zbus::{fdo, Result},
    LoopStatus, Metadata, PlaybackRate, PlaybackStatus, PlayerInterface, Property, RootInterface,
    Server, Time, TrackId, Volume,
};
use tempfile::NamedTempFile;
use tokio::sync::Mutex;
use tracing::info;
use url::Url;

use crate::app::state::{SharedClientState, SharedDaemonState};
use crate::config::Config;
use crate::daemon::state::{NowPlaying, PlaybackState};
use crate::ipc::{DaemonClient, DaemonRequest, DaemonResponse};
use crate::subsonic::auth::generate_auth_params;
use crate::subsonic::models::Child;

const API_VERSION: &str = "1.16.1";
const CLIENT_NAME: &str = "ferrosonic";

/// Edge length, in pixels, of the cover art fetched for MPRIS metadata.
const MPRIS_COVER_SIZE: u32 = 512;

/// Authenticated getCoverArt URL for MPRIS metadata; None when unconfigured.
#[must_use]
pub fn build_cover_art_url(config: &Config, cover_art_id: &str) -> Option<String> {
    if config.base_url.is_empty() || cover_art_id.is_empty() {
        return None;
    }

    let (salt, token) = generate_auth_params(&config.password);
    let mut url = Url::parse(&format!("{}/rest/getCoverArt", config.base_url)).ok()?;

    url.query_pairs_mut()
        .append_pair("id", cover_art_id)
        .append_pair("u", &config.username)
        .append_pair("t", &token)
        .append_pair("s", &salt)
        .append_pair("v", API_VERSION)
        .append_pair("c", CLIENT_NAME);

    Some(url.to_string())
}

const PLAYER_NAME: &str = "ferrosonic";

/// Locally cached cover for one art id, kept so the `file://` URL handed to
/// MPRIS consumers stays valid until the track (and thus its art) changes.
struct CoverCache {
    /// Cover art id the file currently holds.
    cover_id: String,
    /// Tempfile backing the `file://` URL; deletes itself when replaced.
    file: NamedTempFile,
}

/// MPRIS2 player implementation bridging D-Bus to the daemon client.
pub struct MprisPlayer {
    daemon_state: SharedDaemonState,
    client_state: SharedClientState,
    client: Arc<dyn DaemonClient>,
    /// Handle to the tokio runtime captured at construction. zbus invokes
    /// these handlers on its own async-io executor, where `tokio::spawn`
    /// panics with "no reactor"; spawning through this handle runs the
    /// daemon request (which needs tokio I/O) on a real tokio worker.
    rt: tokio::runtime::Handle,
    /// Cover art mirrored to a local file. GNOME Shell's media-controls
    /// widget won't fetch the remote authenticated Subsonic URL, but it
    /// loads a `file://` reliably (same as our desktop notifications).
    cover_cache: Mutex<Option<CoverCache>>,
}

impl MprisPlayer {
    /// Bundle the shared state handles into a player. Must be called from within a tokio runtime; captures its handle for dispatching D-Bus control requests.
    pub fn new(
        daemon_state: SharedDaemonState,
        client_state: SharedClientState,
        client: Arc<dyn DaemonClient>,
    ) -> Self {
        Self {
            daemon_state,
            client_state,
            client,
            rt: tokio::runtime::Handle::current(),
            cover_cache: Mutex::new(None),
        }
    }

    /// Mirror the cover for `cover_id` to a local file and return its
    /// `file://` URL, reusing the cached file when the id is unchanged.
    /// Returns `None` if the fetch yields no bytes or the write fails;
    /// callers then fall back to the remote art URL.
    ///
    /// The lock spans the fetch+write so concurrent metadata pushes for the
    /// same track don't double-fetch or race on the shared tempfile.
    #[allow(clippy::significant_drop_tightening)]
    async fn cover_file_uri(&self, cover_id: &str) -> Option<String> {
        let mut guard = self.cover_cache.lock().await;
        if let Some(cache) = guard.as_ref() {
            if cache.cover_id == cover_id {
                return Some(format!("file://{}", cache.file.path().display()));
            }
        }

        let bytes = match self
            .client
            .request(DaemonRequest::FetchCoverArt {
                id: cover_id.to_string(),
                size: MPRIS_COVER_SIZE,
            })
            .await
        {
            Ok(DaemonResponse::CoverArt(bytes)) if !bytes.is_empty() => bytes,
            _ => return None,
        };

        let file = NamedTempFile::with_prefix("ferrosonic-mpris-").ok()?;
        let path = file.path().to_path_buf();
        tokio::task::spawn_blocking(move || crate::io_util::atomic_write_bytes(&path, &bytes))
            .await
            .ok()?
            .ok()?;

        let uri = format!("file://{}", file.path().display());
        *guard = Some(CoverCache {
            cover_id: cover_id.to_string(),
            file,
        });
        Some(uri)
    }

    /// Dispatch a fire-and-forget daemon request onto the captured tokio runtime. Errors are logged, not propagated, since D-Bus media keys expect no reply.
    fn fire(&self, req: DaemonRequest) {
        let client = self.client.clone();
        self.rt.spawn(async move {
            if let Err(e) = client.request(req).await {
                tracing::warn!("MPRIS request failed: {}", e);
            }
        });
    }

    async fn get_state(&self) -> (NowPlaying, Option<Child>, Config) {
        let ds = self.daemon_state.read().await;
        let now_playing = ds.now_playing.clone();
        let current_song = ds.current_song().cloned();
        let config = ds.config.clone();
        drop(ds);
        (now_playing, current_song, config)
    }
}

impl RootInterface for MprisPlayer {
    async fn raise(&self) -> fdo::Result<()> {
        Ok(())
    }

    async fn quit(&self) -> fdo::Result<()> {
        let mut cs = self.client_state.write().await;
        cs.should_quit = true;
        drop(cs);
        Ok(())
    }

    async fn can_quit(&self) -> fdo::Result<bool> {
        Ok(true)
    }

    async fn fullscreen(&self) -> fdo::Result<bool> {
        Ok(false)
    }

    async fn set_fullscreen(&self, _fullscreen: bool) -> Result<()> {
        Ok(())
    }

    async fn can_set_fullscreen(&self) -> fdo::Result<bool> {
        Ok(false)
    }

    async fn can_raise(&self) -> fdo::Result<bool> {
        Ok(false)
    }

    async fn has_track_list(&self) -> fdo::Result<bool> {
        Ok(false)
    }

    async fn identity(&self) -> fdo::Result<String> {
        Ok("Ferrosonic".to_string())
    }

    async fn desktop_entry(&self) -> fdo::Result<String> {
        Ok("ferrosonic".to_string())
    }

    async fn supported_uri_schemes(&self) -> fdo::Result<Vec<String>> {
        Ok(vec!["http".to_string(), "https".to_string()])
    }

    async fn supported_mime_types(&self) -> fdo::Result<Vec<String>> {
        Ok(vec![
            "audio/mpeg".to_string(),
            "audio/flac".to_string(),
            "audio/ogg".to_string(),
            "audio/wav".to_string(),
            "audio/x-wav".to_string(),
        ])
    }
}

impl PlayerInterface for MprisPlayer {
    async fn next(&self) -> fdo::Result<()> {
        self.fire(DaemonRequest::Next);
        Ok(())
    }

    async fn previous(&self) -> fdo::Result<()> {
        self.fire(DaemonRequest::Previous);
        Ok(())
    }

    async fn pause(&self) -> fdo::Result<()> {
        self.fire(DaemonRequest::Pause);
        Ok(())
    }

    async fn play_pause(&self) -> fdo::Result<()> {
        self.fire(DaemonRequest::TogglePause);
        Ok(())
    }

    async fn stop(&self) -> fdo::Result<()> {
        self.fire(DaemonRequest::Stop);
        Ok(())
    }

    async fn play(&self) -> fdo::Result<()> {
        self.fire(DaemonRequest::Resume);
        Ok(())
    }

    // i64 micros->f64: precision loss only past 2^52us (~142yr); irrelevant for a seek offset.
    #[allow(clippy::cast_precision_loss)]
    async fn seek(&self, offset: Time) -> fdo::Result<()> {
        let offset_secs = offset.as_micros() as f64 / 1_000_000.0;
        self.fire(DaemonRequest::SeekRelative(offset_secs));
        Ok(())
    }

    // i64 micros->f64: precision loss only past 2^52us (~142yr); irrelevant for a track position.
    #[allow(clippy::cast_precision_loss)]
    async fn set_position(&self, _track_id: TrackId, position: Time) -> fdo::Result<()> {
        let position_secs = position.as_micros() as f64 / 1_000_000.0;
        self.fire(DaemonRequest::Seek(position_secs));
        Ok(())
    }

    async fn open_uri(&self, _uri: String) -> fdo::Result<()> {
        Ok(())
    }

    async fn playback_status(&self) -> fdo::Result<PlaybackStatus> {
        let (now_playing, _, _) = self.get_state().await;
        Ok(match now_playing.state {
            PlaybackState::Playing => PlaybackStatus::Playing,
            PlaybackState::Paused => PlaybackStatus::Paused,
            PlaybackState::Stopped => PlaybackStatus::Stopped,
        })
    }

    async fn loop_status(&self) -> fdo::Result<LoopStatus> {
        Ok(LoopStatus::None)
    }

    async fn set_loop_status(&self, _loop_status: LoopStatus) -> Result<()> {
        Ok(())
    }

    async fn rate(&self) -> fdo::Result<PlaybackRate> {
        Ok(1.0)
    }

    async fn set_rate(&self, _rate: PlaybackRate) -> Result<()> {
        Ok(())
    }

    async fn shuffle(&self) -> fdo::Result<bool> {
        Ok(false)
    }

    async fn set_shuffle(&self, _shuffle: bool) -> Result<()> {
        Ok(())
    }

    async fn metadata(&self) -> fdo::Result<Metadata> {
        let (_now_playing, current_song, config) = self.get_state().await;

        let mut metadata = Metadata::new();

        if let Some(song) = current_song {
            metadata.set_trackid(
                Some(TrackId::try_from(format!("/org/mpris/MediaPlayer2/Track/{}", song.id)).ok())
                    .flatten(),
            );
            metadata.set_title(Some(song.title));
            metadata.set_artist(song.artist.map(|a| vec![a]));
            metadata.set_album(song.album);

            if let Some(duration) = song.duration {
                metadata.set_length(Some(Time::from_micros(i64::from(duration) * 1_000_000)));
            }

            if let Some(track) = song.track {
                metadata.set_track_number(Some(track));
            }

            if let Some(disc) = song.disc_number {
                metadata.set_disc_number(Some(disc));
            }

            // Remote (authenticated) URL only. The local file:// swap happens in
            // `update_mpris_properties`, which runs on the tokio runtime; doing
            // the fetch here would run on zbus's executor where daemon I/O has
            // no reactor (the same reason `fire` exists).
            if let Some(ref cover_art_id) = song.cover_art {
                if let Some(cover_url) = build_cover_art_url(&config, cover_art_id) {
                    metadata.set_art_url(Some(cover_url));
                }
            }
        }

        Ok(metadata)
    }

    async fn volume(&self) -> fdo::Result<Volume> {
        Ok(1.0)
    }

    // f64->i32 `as` saturates; volume is the 0.0..=1.0 MPRIS range, so 0..=100.
    #[allow(clippy::cast_possible_truncation)]
    async fn set_volume(&self, volume: Volume) -> Result<()> {
        let volume_int = (volume * 100.0) as i32;
        self.fire(DaemonRequest::SetVolume(volume_int));
        Ok(())
    }

    // f64->i64 `as` saturates; position*1e6 micros is bounded by track length.
    #[allow(clippy::cast_possible_truncation)]
    async fn position(&self) -> fdo::Result<Time> {
        let (now_playing, _, _) = self.get_state().await;
        Ok(Time::from_micros(
            (now_playing.position * 1_000_000.0) as i64,
        ))
    }

    async fn minimum_rate(&self) -> fdo::Result<PlaybackRate> {
        Ok(1.0)
    }

    async fn maximum_rate(&self) -> fdo::Result<PlaybackRate> {
        Ok(1.0)
    }

    async fn can_go_next(&self) -> fdo::Result<bool> {
        let ds = self.daemon_state.read().await;
        Ok(ds.queue_position.is_some_and(|p| p + 1 < ds.queue.len()))
    }

    async fn can_go_previous(&self) -> fdo::Result<bool> {
        let ds = self.daemon_state.read().await;
        Ok(ds.queue_position.is_some_and(|p| p > 0))
    }

    async fn can_play(&self) -> fdo::Result<bool> {
        let ds = self.daemon_state.read().await;
        Ok(!ds.queue.is_empty())
    }

    async fn can_pause(&self) -> fdo::Result<bool> {
        Ok(true)
    }

    async fn can_seek(&self) -> fdo::Result<bool> {
        Ok(true)
    }

    async fn can_control(&self) -> fdo::Result<bool> {
        Ok(true)
    }
}

/// Register the MPRIS2 player on the session bus.
///
/// # Errors
/// Returns an error if the D-Bus call fails.
pub async fn start_mpris_server(
    daemon_state: SharedDaemonState,
    client_state: SharedClientState,
    client: Arc<dyn DaemonClient>,
) -> Result<Server<MprisPlayer>> {
    info!("Starting MPRIS2 server");

    let player = MprisPlayer::new(daemon_state, client_state, client);
    let server = Server::new(PLAYER_NAME, player).await?;

    info!(
        "MPRIS2 server started as org.mpris.MediaPlayer2.{}",
        PLAYER_NAME
    );
    Ok(server)
}

/// Snapshot of the values that `update_mpris_properties` will push.
/// Extracted so tests can verify the construction without D-Bus.
#[derive(Debug)]
pub struct MprisPropertySnapshot {
    /// Current playback status.
    pub playback: PlaybackStatus,
    /// Whether a next track exists.
    pub can_go_next: bool,
    /// Whether a previous track exists.
    pub can_go_prev: bool,
    /// Whether playback is possible (queue non-empty). Event-driven MPRIS
    /// consumers (e.g. GNOME Shell) cache `CanPlay` from the initial read
    /// and only refresh it via `PropertiesChanged`, so it must be pushed.
    pub can_play: bool,
    /// Cover art id of the current song, if any. Used to mirror the art to a
    /// local file so the pushed `Metadata` carries a loadable `file://` URL.
    pub cover_id: Option<String>,
    /// Track metadata, when a song is loaded.
    pub metadata: Option<Metadata>,
}

/// Pure: builds the property snapshot from daemon state.
pub async fn build_property_snapshot(daemon_state: &SharedDaemonState) -> MprisPropertySnapshot {
    let (playback, can_go_next, can_go_prev, can_play, current_song, config) = {
        let ds = daemon_state.read().await;
        let pb = match ds.now_playing.state {
            PlaybackState::Playing => PlaybackStatus::Playing,
            PlaybackState::Paused => PlaybackStatus::Paused,
            PlaybackState::Stopped => PlaybackStatus::Stopped,
        };
        let cgn = ds.queue_position.is_some_and(|p| p + 1 < ds.queue.len());
        let cgp = ds.queue_position.is_some_and(|p| p > 0);
        let cp = !ds.queue.is_empty();
        (
            pb,
            cgn,
            cgp,
            cp,
            ds.current_song().cloned(),
            ds.config.clone(),
        )
    };

    let cover_id = current_song.as_ref().and_then(Child::cover_id);
    let metadata = current_song.map(|song| build_metadata_for(&song, &config));

    MprisPropertySnapshot {
        playback,
        can_go_next,
        can_go_prev,
        can_play,
        cover_id,
        metadata,
    }
}

fn build_metadata_for(song: &Child, config: &Config) -> Metadata {
    let mut metadata = Metadata::new();
    metadata.set_trackid(
        Some(TrackId::try_from(format!("/org/mpris/MediaPlayer2/Track/{}", song.id)).ok())
            .flatten(),
    );
    metadata.set_title(Some(song.title.clone()));
    metadata.set_artist(song.artist.clone().map(|a| vec![a]));
    metadata.set_album(song.album.clone());

    if let Some(duration) = song.duration {
        metadata.set_length(Some(Time::from_micros(i64::from(duration) * 1_000_000)));
    }

    if let Some(ref cover_art_id) = song.cover_art {
        if let Some(cover_url) = build_cover_art_url(config, cover_art_id) {
            metadata.set_art_url(Some(cover_url));
        }
    }

    metadata
}

/// Releases the daemon read lock before the D-Bus await so a slow
/// D-Bus doesn't block the render-path write lock.
///
/// # Errors
/// Returns an error if the D-Bus call fails.
pub async fn update_mpris_properties(
    server: &Server<MprisPlayer>,
    daemon_state: &SharedDaemonState,
) -> Result<()> {
    let snap = build_property_snapshot(daemon_state).await;

    server
        .properties_changed([
            Property::PlaybackStatus(snap.playback),
            Property::CanGoNext(snap.can_go_next),
            Property::CanGoPrevious(snap.can_go_prev),
            Property::CanPlay(snap.can_play),
        ])
        .await?;

    if let Some(mut metadata) = snap.metadata {
        // Swap the remote art URL for a local file:// the widget can load.
        if let Some(cid) = &snap.cover_id {
            if let Some(file_url) = server.imp().cover_file_uri(cid).await {
                metadata.set_art_url(Some(file_url));
            }
        }
        server
            .properties_changed([Property::Metadata(metadata)])
            .await?;
    }

    Ok(())
}
