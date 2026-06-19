//! IPC protocol: length-prefixed JSON over a Unix socket.

use serde::{Deserialize, Serialize};

use crate::config::{Config, RepeatMode};
use crate::daemon::state::{DaemonState, NowPlaying};
use crate::secret::{deserialize_secret, serialize_revealed, Secret};
use crate::subsonic::models::{Album, Artist, Child, Playlist, SearchResult3};

/// Client-to-daemon command sent over the IPC socket.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DaemonRequest {
    /// Pause playback.
    Pause,
    /// Resume paused playback.
    Resume,
    /// Toggle between playing and paused.
    TogglePause,
    /// Stop playback and unload the current track.
    Stop,
    /// Seek to an absolute position in seconds.
    Seek(f64),
    /// Seek relative to the current position, in seconds.
    SeekRelative(f64),
    /// Skip to the next queue entry.
    Next,
    /// Skip to the previous queue entry.
    Previous,
    /// Set playback volume as a percentage (0-100).
    SetVolume(i32),

    /// Add songs to the queue using the given placement mode.
    EnqueueSongs {
        /// Songs to enqueue, in order.
        songs: Vec<Child>,
        /// Where the songs land relative to the existing queue.
        mode: EnqueueMode,
    },
    /// Jump playback to the queue entry at this index.
    PlayQueueIndex(usize),
    /// Remove the queue entry at this index.
    RemoveFromQueue(usize),
    /// Empty the queue and stop playback.
    ClearQueue,
    /// Shuffle the unplayed remainder of the queue.
    ShuffleQueue,
    /// Replace the queue with the whole library, shuffled.
    ShuffleLibrary,
    /// Move a queue entry from one index to another.
    MoveQueueItem {
        /// Index the entry currently occupies.
        from: usize,
        /// Index the entry moves to.
        to: usize,
    },
    /// Drain entries before `queue_position` (the played-history half).
    ClearQueueHistory,

    /// Re-fetch the starred-songs list from the server.
    RefreshStarred,
    /// Re-fetch the random-songs list from the server.
    RefreshRandom,
    /// Re-fetch the artist index from the server.
    RefreshArtists,
    /// Re-fetch the playlist list from the server.
    RefreshPlaylists,
    /// Create a server-side playlist from an ordered list of song IDs.
    CreatePlaylist {
        /// Playlist name as typed by the user.
        name: String,
        /// Song IDs in queue order.
        song_ids: Vec<String>,
    },
    /// Star or unstar the song with this ID.
    ToggleStarSong(String),
    /// Fetch the albums of the artist with this ID.
    LoadArtist(String),
    /// Fetch the entire album library for the flat album-list view.
    LoadAllAlbums,
    /// Fetch the songs of the album with this ID.
    LoadAlbum(String),
    /// Fetch the songs of the playlist with this ID.
    LoadPlaylist(String),
    /// Run a server-side search across artists, albums, and songs.
    Search {
        /// Search term.
        query: String,
        /// Maximum artists to return.
        artist_count: u32,
        /// Maximum albums to return.
        album_count: u32,
        /// Maximum songs to return.
        song_count: u32,
    },

    /// Persist new Subsonic server credentials and reconnect.
    UpdateServerConfig {
        /// Server base URL, scheme included.
        base_url: String,
        /// Subsonic account username.
        username: String,
        /// Subsonic account password.
        #[serde(
            serialize_with = "serialize_revealed",
            deserialize_with = "deserialize_secret"
        )]
        password: Secret,
    },
    /// Probe a server with these credentials without persisting them.
    TestServerConnection {
        /// Server base URL, scheme included.
        base_url: String,
        /// Subsonic account username.
        username: String,
        /// Subsonic account password.
        #[serde(
            serialize_with = "serialize_revealed",
            deserialize_with = "deserialize_secret"
        )]
        password: Secret,
    },
    /// Switch the UI theme by name and persist the choice.
    SetTheme(String),
    /// Enable or disable the cava visualizer.
    SetCavaEnabled(bool),
    /// Set the cava visualizer height in rows.
    SetCavaSize(u8),
    /// Takes effect on the next TUI launch; the running daemon is unaffected.
    SetDaemonEnabled(bool),
    /// Enable or disable auto-continue past the end of the queue.
    SetAutoContinue(bool),
    /// Enable or disable reporting plays to the server (scrobbling).
    SetScrobble(bool),
    /// Set the repeat mode and persist the choice.
    SetRepeatMode(RepeatMode),
    /// Enable or disable cover art rendering.
    SetCoverArtEnabled(bool),
    /// Set the cover art pane width in columns.
    SetCoverArtSize(u8),
    /// Fetch cover art bytes for an item, scaled to `size` pixels.
    FetchCoverArt {
        /// Cover art ID from the owning item.
        id: String,
        /// Requested edge length in pixels.
        size: u32,
    },

    /// Register this connection for `DaemonEvent` broadcasts.
    Subscribe,
    /// Request a full `DaemonState` snapshot.
    Snapshot,
    /// Shut the daemon down.
    Shutdown,
    /// Liveness probe; answered with `Pong`.
    Ping,
}

/// Placement of newly enqueued songs relative to the existing queue.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EnqueueMode {
    /// Replace the queue, optionally starting playback at an index.
    Replace {
        /// Index to start playing from, or `None` to stay stopped.
        play_from: Option<usize>,
    },
    /// Append after the last queue entry.
    Append,
    /// Insert directly after this index.
    InsertAfter(usize),
}

/// Daemon-to-client reply to a single `DaemonRequest`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DaemonResponse {
    /// Request succeeded with nothing to return.
    Ok,
    /// Request failed; the message is human-readable.
    Err(String),
    /// Albums of a requested artist.
    ArtistAlbums(Vec<Album>),
    /// The entire album library for the flat album-list view.
    AllAlbums(Vec<Album>),
    /// Songs of a requested album.
    AlbumSongs(Vec<Child>),
    /// Songs of a requested playlist.
    PlaylistSongs(Vec<Child>),
    /// Outcome of `TestServerConnection`.
    ConnectionTestResult {
        /// Whether the probe reached and authenticated with the server.
        ok: bool,
        /// Human-readable probe outcome.
        message: String,
    },
    /// Number of history entries removed by `ClearQueueHistory`.
    HistoryCleared(usize),
    /// Full daemon state, boxed for frame-size economy.
    Snapshot(Box<DaemonState>),
    /// Results of a `Search` request.
    SearchResults(SearchResult3),
    /// Raw cover art image bytes.
    CoverArt(Vec<u8>),
    /// Reply to `Ping`.
    Pong,
}

/// Server-pushed state-change broadcast. Variants carry the new value
/// inline so subscribers update their mirror without a follow-up RPC.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DaemonEvent {
    /// Queue contents or playback position changed.
    QueueChanged {
        /// New queue contents, in order.
        queue: Vec<Child>,
        /// Index of the playing entry, if any.
        position: Option<usize>,
    },
    /// Song / playback state / sample rate change. Position-only ticks
    /// use `PositionTick` to avoid event spam.
    NowPlayingChanged(NowPlaying),
    /// Playback position update in seconds.
    PositionTick(f64),
    /// New starred-songs list.
    StarredChanged(Vec<Child>),
    /// Star state of one song changed.
    SongStarChanged {
        /// ID of the affected song.
        id: String,
        /// New star state.
        starred: bool,
    },
    /// New random-songs list.
    RandomChanged(Vec<Child>),
    /// New artist index.
    ArtistsChanged(Vec<Artist>),
    /// Album list of one artist changed.
    AlbumsChanged {
        /// ID of the affected artist.
        artist_id: String,
        /// New album list.
        albums: Vec<Album>,
    },
    /// Song list of one album changed.
    AlbumSongsChanged {
        /// ID of the affected album.
        album_id: String,
        /// New song list.
        songs: Vec<Child>,
    },
    /// New playlist list.
    PlaylistsChanged(Vec<Playlist>),
    /// Song list of one playlist changed.
    PlaylistSongsChanged {
        /// ID of the affected playlist.
        playlist_id: String,
        /// New song list.
        songs: Vec<Child>,
    },
    /// User-facing notification for the TUI footer.
    Notification {
        /// Notification text.
        message: String,
        /// Whether to style the notification as an error.
        is_error: bool,
    },
    /// Repeat mode changed.
    RepeatModeChanged(RepeatMode),
    /// Persisted configuration changed.
    ConfigChanged(Config),
    /// Daemon is shutting down; subscribers should disconnect.
    Shutdown,
    /// Opt-in pull-style alternative to the bulk ArtistsChanged etc events.
    LibraryVersionChanged(u64),
}

/// Error surface of the IPC client and server transport.
#[derive(Debug, thiserror::Error)]
pub enum IpcError {
    /// Daemon answered with `DaemonResponse::Err`.
    #[error("daemon error: {0}")]
    Daemon(String),
    /// Connection closed while a reply was pending.
    #[error("daemon disconnected")]
    Disconnected,
    /// Socket I/O failed.
    #[error("transport: {0}")]
    Transport(#[from] std::io::Error),
    /// JSON encode or decode failed.
    #[error("serialize: {0}")]
    Serialize(#[from] serde_json::Error),
}
