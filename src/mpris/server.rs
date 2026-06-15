//! MPRIS2 D-Bus server.

use std::sync::Arc;

use mpris_server::{
    zbus::{fdo, Result},
    LoopStatus, Metadata, PlaybackRate, PlaybackStatus, PlayerInterface, Property, RootInterface,
    Server, Time, TrackId, Volume,
};
use tracing::info;
use url::Url;

use crate::app::state::{SharedClientState, SharedDaemonState};
use crate::config::Config;
use crate::daemon::state::{NowPlaying, PlaybackState};
use crate::ipc::{DaemonClient, DaemonRequest};
use crate::subsonic::auth::generate_auth_params;
use crate::subsonic::models::Child;

const API_VERSION: &str = "1.16.1";
const CLIENT_NAME: &str = "ferrosonic";

/// Authenticated getCoverArt URL for MPRIS metadata; None when unconfigured.
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
        }
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

    async fn seek(&self, offset: Time) -> fdo::Result<()> {
        let offset_secs = offset.as_micros() as f64 / 1_000_000.0;
        self.fire(DaemonRequest::SeekRelative(offset_secs));
        Ok(())
    }

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
                metadata.set_length(Some(Time::from_micros(duration as i64 * 1_000_000)));
            }

            if let Some(track) = song.track {
                metadata.set_track_number(Some(track));
            }

            if let Some(disc) = song.disc_number {
                metadata.set_disc_number(Some(disc));
            }

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

    async fn set_volume(&self, volume: Volume) -> Result<()> {
        let volume_int = (volume * 100.0) as i32;
        self.fire(DaemonRequest::SetVolume(volume_int));
        Ok(())
    }

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
        Ok(ds
            .queue_position
            .map(|p| p + 1 < ds.queue.len())
            .unwrap_or(false))
    }

    async fn can_go_previous(&self) -> fdo::Result<bool> {
        let ds = self.daemon_state.read().await;
        Ok(ds.queue_position.map(|p| p > 0).unwrap_or(false))
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
    /// Track metadata, when a song is loaded.
    pub metadata: Option<Metadata>,
}

/// Pure: builds the property snapshot from daemon state.
pub async fn build_property_snapshot(daemon_state: &SharedDaemonState) -> MprisPropertySnapshot {
    let (playback, can_go_next, can_go_prev, current_song, config) = {
        let ds = daemon_state.read().await;
        let pb = match ds.now_playing.state {
            PlaybackState::Playing => PlaybackStatus::Playing,
            PlaybackState::Paused => PlaybackStatus::Paused,
            PlaybackState::Stopped => PlaybackStatus::Stopped,
        };
        let cgn = ds
            .queue_position
            .map(|p| p + 1 < ds.queue.len())
            .unwrap_or(false);
        let cgp = ds.queue_position.map(|p| p > 0).unwrap_or(false);
        (pb, cgn, cgp, ds.current_song().cloned(), ds.config.clone())
    };

    let metadata = current_song.map(|song| build_metadata_for(&song, &config));

    MprisPropertySnapshot {
        playback,
        can_go_next,
        can_go_prev,
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
        metadata.set_length(Some(Time::from_micros(duration as i64 * 1_000_000)));
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
        ])
        .await?;

    if let Some(metadata) = snap.metadata {
        server
            .properties_changed([Property::Metadata(metadata)])
            .await?;
    }

    Ok(())
}
