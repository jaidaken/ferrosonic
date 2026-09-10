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
use crate::config::{Config, RepeatMode};
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
    /// Last volume applied through MPRIS, in percent (0..=100). mpv owns the
    /// authoritative value; caching it here keeps the `Volume` getter
    /// consistent with `SetVolume` for the session instead of always 1.0.
    volume: std::sync::atomic::AtomicI32,
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
            volume: std::sync::atomic::AtomicI32::new(100),
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

    /// `file://` URL for `cover_id` if it has already been mirrored locally.
    /// Synchronous (no fetch/await), so the `metadata()` getter's future stays
    /// `Sync` as `mpris-server` requires; the push path does the mirroring.
    // The guard must outlive `cache`, which borrows into it; an early drop
    // would not compile, so tightening is borrow-blocked.
    #[allow(clippy::significant_drop_tightening)]
    fn cached_cover_uri(&self, cover_id: &str) -> Option<String> {
        let guard = self.cover_cache.try_lock().ok()?;
        let cache = guard.as_ref()?;
        (cache.cover_id == cover_id).then(|| format!("file://{}", cache.file.path().display()))
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
        let (_now_playing, _current_song, config) = self.get_state().await;
        Ok(repeat_to_loop(config.repeat_mode))
    }

    async fn set_loop_status(&self, loop_status: LoopStatus) -> Result<()> {
        self.fire(DaemonRequest::SetRepeatMode(loop_to_repeat(loop_status)));
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
        let Some(song) = current_song else {
            return Ok(Metadata::new());
        };
        // Build the shared fields, then replace the authenticated remote art
        // URL with a locally mirrored `file://` one. The remote URL embeds a
        // reusable `t`/`s` credential pair, which must never reach the bus.
        let mut metadata = build_metadata_for(&song, &config);
        metadata.set_art_url(None::<String>);
        if let Some(cover_id) = song.cover_id() {
            if let Some(file_url) = self.cached_cover_uri(&cover_id) {
                metadata.set_art_url(Some(file_url));
            }
        }
        Ok(metadata)
    }

    async fn volume(&self) -> fdo::Result<Volume> {
        Ok(f64::from(self.volume.load(std::sync::atomic::Ordering::Relaxed)) / 100.0)
    }

    // f64->i32 `as` truncates; the value is clamped to 0.0..=1.0 first.
    #[allow(clippy::cast_possible_truncation)]
    async fn set_volume(&self, volume: Volume) -> Result<()> {
        // Clamp a misbehaving client to MPRIS's 0.0..=1.0 range before
        // rounding to mpv's 0..=100 percent.
        let clamped = volume.clamp(0.0, 1.0);
        let volume_int = (clamped * 100.0).round() as i32;
        self.volume
            .store(volume_int, std::sync::atomic::Ordering::Relaxed);
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
    /// Repeat mode mirrored to MPRIS `LoopStatus`.
    pub loop_status: LoopStatus,
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
    let loop_status = repeat_to_loop(config.repeat_mode);

    MprisPropertySnapshot {
        playback,
        can_go_next,
        can_go_prev,
        can_play,
        cover_id,
        metadata,
        loop_status,
    }
}

/// Encode a Subsonic song id into a D-Bus object-path-safe track suffix.
///
/// D-Bus path elements permit only `[A-Za-z0-9_]`, but Subsonic ids are opaque
/// and routinely contain `-`, `.`, `:`, and other punctuation, so
/// `TrackId::try_from` would reject them and `mpris:trackid` would be silently
/// dropped. Hex-encoding the UTF-8 bytes yields a stable, valid, reversible
/// suffix for any id.
fn encode_track_id(song_id: &str) -> String {
    use std::fmt::Write as _;
    let mut out = String::with_capacity(song_id.len() * 2);
    for byte in song_id.as_bytes() {
        // Writing into a String cannot fail; the `Result` is discarded.
        let _ = write!(out, "{byte:02x}");
    }
    out
}

/// Map ferrosonic's repeat mode to MPRIS `LoopStatus`.
const fn repeat_to_loop(mode: RepeatMode) -> LoopStatus {
    match mode {
        RepeatMode::Off => LoopStatus::None,
        RepeatMode::All => LoopStatus::Playlist,
        RepeatMode::One => LoopStatus::Track,
    }
}

/// Map MPRIS `LoopStatus` back to ferrosonic's repeat mode.
const fn loop_to_repeat(status: LoopStatus) -> RepeatMode {
    match status {
        LoopStatus::None => RepeatMode::Off,
        LoopStatus::Playlist => RepeatMode::All,
        LoopStatus::Track => RepeatMode::One,
    }
}

fn build_metadata_for(song: &Child, config: &Config) -> Metadata {
    let mut metadata = Metadata::new();
    metadata.set_trackid(
        TrackId::try_from(format!(
            "/org/mpris/MediaPlayer2/Track/{}",
            encode_track_id(&song.id)
        ))
        .ok(),
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

    metadata.set_track_number(song.track);
    metadata.set_disc_number(song.disc_number);

    if let Some(rating) = song.user_rating {
        // MPRIS's xesam:userRating is 0.0-1.0; Subsonic's is an integer 1-5.
        metadata.set_user_rating(Some(f64::from(rating) / 5.0));
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
            Property::LoopStatus(snap.loop_status),
        ])
        .await?;

    if let Some(metadata) = snap.metadata {
        push_metadata(server, metadata, snap.cover_id.as_deref()).await?;
    }

    Ok(())
}

/// Push a rating event directly, even if the TUI event pump has not yet applied
/// it to the local state mirror. Rating changes must reach paused-track widgets.
pub(crate) async fn update_mpris_rating(
    server: &Server<MprisPlayer>,
    daemon_state: &SharedDaemonState,
    id: &str,
    rating: Option<u8>,
) -> Result<()> {
    if let Some((metadata, cover_id)) = build_rating_metadata(daemon_state, id, rating).await {
        push_metadata(server, metadata, cover_id.as_deref()).await?;
    }
    Ok(())
}

async fn build_rating_metadata(
    daemon_state: &SharedDaemonState,
    id: &str,
    rating: Option<u8>,
) -> Option<(Metadata, Option<String>)> {
    let state = daemon_state.read().await;
    let mut song = state.current_song()?.clone();
    if song.id != id {
        return None;
    }
    song.user_rating = rating;
    Some((build_metadata_for(&song, &state.config), song.cover_id()))
}

async fn push_metadata(
    server: &Server<MprisPlayer>,
    mut metadata: Metadata,
    cover_id: Option<&str>,
) -> Result<()> {
    // Never publish the authenticated remote Subsonic art URL: it carries a
    // reusable `t`/`s` credential pair. Publish only a locally mirrored
    // `file://` URL, or no art at all when the mirror cannot be produced.
    metadata.set_art_url(None::<String>);
    if let Some(cid) = cover_id {
        if let Some(file_url) = server.imp().cover_file_uri(cid).await {
            metadata.set_art_url(Some(file_url));
        }
    }
    server
        .properties_changed([Property::Metadata(metadata)])
        .await
}

#[cfg(test)]
mod rating_event_tests {
    use super::*;

    #[tokio::test]
    async fn rating_event_metadata_does_not_depend_on_tui_pump_order() {
        let state = crate::app::state::new_shared_daemon_state(Config::new());
        {
            let mut state = state.write().await;
            let song = Child {
                id: "rated".into(),
                user_rating: Some(1),
                ..Default::default()
            };
            state.queue = vec![song.clone()];
            state.queue_position = Some(0);
            state.now_playing.song = Some(song);
            state.now_playing.state = PlaybackState::Paused;
        }
        for rating in [Some(5), None] {
            let (metadata, _) = build_rating_metadata(&state, "rated", rating)
                .await
                .unwrap();
            assert_eq!(metadata.user_rating(), rating.map(|r| f64::from(r) / 5.0));
        }
        assert!(build_rating_metadata(&state, "other", Some(3))
            .await
            .is_none());
        assert_eq!(
            state.read().await.queue[0].user_rating,
            Some(1),
            "MPRIS must not race the TUI by mutating its state mirror"
        );
    }

    #[test]
    fn track_id_encoding_is_path_safe_for_punctuated_ids() {
        // D-Bus path elements reject punctuation; hex-encoding keeps every id
        // valid instead of silently dropping mpris:trackid.
        assert_eq!(encode_track_id("abc"), "616263");
        assert_eq!(encode_track_id("track-1"), "747261636b2d31");

        let song = Child {
            id: "f47ac10b-58cc-4372-a567-0e02b2c3d479".into(),
            title: "UUID track".into(),
            ..Default::default()
        };
        let metadata = build_metadata_for(&song, &Config::new());
        assert!(
            metadata.trackid().is_some(),
            "a hyphenated/UUID id must still yield a valid trackid"
        );
    }
}
