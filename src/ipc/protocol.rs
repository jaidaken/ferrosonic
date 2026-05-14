//! IPC protocol: length-prefixed JSON over a Unix socket.

#![allow(dead_code)]
#![allow(clippy::large_enum_variant)]

use serde::{Deserialize, Serialize};

use crate::app::state::NowPlaying;
use crate::config::{Config, RepeatMode};
use crate::daemon::state::DaemonState;
use crate::secret::{deserialize_secret, serialize_revealed, Secret};
use crate::subsonic::models::{Album, Artist, Child, Playlist, SearchResult3};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DaemonRequest {
    Pause,
    Resume,
    TogglePause,
    Stop,
    Seek(f64),
    SeekRelative(f64),
    Next,
    Previous,
    SetVolume(i32),

    EnqueueSongs {
        songs: Vec<Child>,
        mode: EnqueueMode,
    },
    PlayQueueIndex(usize),
    RemoveFromQueue(usize),
    ClearQueue,
    ShuffleQueue,
    ShuffleLibrary,
    MoveQueueItem {
        from: usize,
        to: usize,
    },
    /// Drain entries before `queue_position` (the played-history half).
    ClearQueueHistory,

    RefreshStarred,
    RefreshRandom,
    RefreshArtists,
    RefreshPlaylists,
    ToggleStarSong(String),
    LoadArtist(String),
    LoadAlbum(String),
    LoadPlaylist(String),
    Search {
        query: String,
        artist_count: u32,
        album_count: u32,
        song_count: u32,
    },

    UpdateServerConfig {
        base_url: String,
        username: String,
        #[serde(
            serialize_with = "serialize_revealed",
            deserialize_with = "deserialize_secret"
        )]
        password: Secret,
    },
    TestServerConnection {
        base_url: String,
        username: String,
        #[serde(
            serialize_with = "serialize_revealed",
            deserialize_with = "deserialize_secret"
        )]
        password: Secret,
    },
    SetTheme(String),
    SetCavaEnabled(bool),
    SetCavaSize(u8),
    /// Takes effect on the next TUI launch; the running daemon is unaffected.
    SetDaemonEnabled(bool),
    SetAutoContinue(bool),
    SetRepeatMode(RepeatMode),
    SetCoverArtEnabled(bool),
    SetCoverArtSize(u8),
    FetchCoverArt {
        id: String,
        size: u32,
    },

    Subscribe,
    Snapshot,
    Shutdown,
    Ping,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EnqueueMode {
    Replace { play_from: Option<usize> },
    Append,
    InsertAfter(usize),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DaemonResponse {
    Ok,
    Err(String),
    ArtistAlbums(Vec<Album>),
    AlbumSongs(Vec<Child>),
    PlaylistSongs(Vec<Child>),
    ConnectionTestResult { ok: bool, message: String },
    HistoryCleared(usize),
    Snapshot(Box<DaemonState>),
    SearchResults(SearchResult3),
    CoverArt(Vec<u8>),
    Pong,
}

/// Server-pushed state-change broadcast. Variants carry the new value
/// inline so subscribers update their mirror without a follow-up RPC.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DaemonEvent {
    QueueChanged {
        queue: Vec<Child>,
        position: Option<usize>,
    },
    /// Song / playback state / sample rate change. Position-only ticks
    /// use `PositionTick` to avoid event spam.
    NowPlayingChanged(NowPlaying),
    PositionTick(f64),
    StarredChanged(Vec<Child>),
    SongStarChanged {
        id: String,
        starred: bool,
    },
    RandomChanged(Vec<Child>),
    ArtistsChanged(Vec<Artist>),
    AlbumsChanged {
        artist_id: String,
        albums: Vec<Album>,
    },
    AlbumSongsChanged {
        album_id: String,
        songs: Vec<Child>,
    },
    PlaylistsChanged(Vec<Playlist>),
    PlaylistSongsChanged {
        playlist_id: String,
        songs: Vec<Child>,
    },
    Notification {
        message: String,
        is_error: bool,
    },
    RepeatModeChanged(RepeatMode),
    ConfigChanged(Config),
    Shutdown,
    /// Opt-in pull-style alternative to the bulk ArtistsChanged etc events.
    LibraryVersionChanged(u64),
}

#[derive(Debug, thiserror::Error)]
pub enum IpcError {
    #[error("daemon error: {0}")]
    Daemon(String),
    #[error("daemon disconnected")]
    Disconnected,
    #[error("transport: {0}")]
    Transport(#[from] std::io::Error),
    #[error("serialize: {0}")]
    Serialize(#[from] serde_json::Error),
}
