//! Wire protocol between TUI client and ferrosonicd daemon.
//!
//! All three top-level types (`DaemonRequest`, `DaemonResponse`, `DaemonEvent`)
//! are tagged enums serializable with serde. The framing is length-prefixed
//! JSON over `tokio::net::UnixStream` (see `src/ipc/frame.rs` from phase 4).
//!
//! Today only `InProcessClient` consumes these — the daemon and TUI live in
//! the same process. The types are `Serialize`/`Deserialize`-ready already
//! so phase 4 can light up the socket without protocol changes.

// Phase 2.1 stages the protocol types only; consumers (DaemonClient,
// InProcessClient, DaemonCore) come in subsequent commits.
#![allow(dead_code)]

use serde::{Deserialize, Serialize};

use crate::config::Config;
use crate::subsonic::models::{Album, Child};

// ────────────────────────────────────────────────────────────────────────────
// Requests: commands client sends to daemon
// ────────────────────────────────────────────────────────────────────────────

/// A command from the TUI client to the daemon. The daemon replies with a
/// matching `DaemonResponse` and, where state changed, broadcasts a
/// `DaemonEvent` to all subscribers.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum DaemonRequest {
    // ── Audio control ───────────────────────────────────────────────────
    /// Pause playback. No-op if not playing.
    Pause,
    /// Resume playback. No-op if not paused.
    Resume,
    /// Toggle play/pause.
    TogglePause,
    /// Stop playback and clear the queue.
    Stop,
    /// Seek to absolute position in seconds.
    Seek(f64),
    /// Seek by a relative offset in seconds.
    SeekRelative(f64),
    /// Skip to the next track in the queue.
    Next,
    /// Skip to the previous track (or restart current if > 3s in).
    Previous,
    /// Set volume (0-100).
    SetVolume(i32),

    // ── Queue operations ────────────────────────────────────────────────
    /// Enqueue a list of songs. `mode` controls whether they replace the
    /// current queue, append to it, or insert at a specific index.
    EnqueueSongs {
        songs: Vec<Child>,
        mode: EnqueueMode,
    },
    /// Play the song at the given queue index.
    PlayQueueIndex(usize),
    /// Remove the song at the given queue index.
    RemoveFromQueue(usize),
    /// Empty the queue and stop playback.
    ClearQueue,
    /// Shuffle the current queue (preserving the currently-playing track).
    ShuffleQueue,

    // ── Library operations ──────────────────────────────────────────────
    /// Refetch starred songs from the Subsonic server.
    RefreshStarred,
    /// Refetch the random-songs roll from the Subsonic server.
    RefreshRandom,
    /// Refetch the artist tree from the Subsonic server.
    RefreshArtists,
    /// Refetch the playlist list from the Subsonic server.
    RefreshPlaylists,
    /// Lazily load albums for an artist into the cache.
    LoadArtist(String),
    /// Lazily load songs for an album into the cache.
    LoadAlbum(String),
    /// Lazily load songs for a playlist into the cache.
    LoadPlaylist(String),

    // ── Config operations ───────────────────────────────────────────────
    /// Update the Subsonic server configuration and persist it. Daemon
    /// reinitialises its `SubsonicClient` and re-fetches initial data.
    UpdateServerConfig {
        base_url: String,
        username: String,
        password: String,
    },
    /// Test a candidate server config without persisting.
    TestServerConnection {
        base_url: String,
        username: String,
        password: String,
    },
    /// Set the active theme by name and persist.
    SetTheme(String),
    /// Enable/disable cava and persist.
    SetCavaEnabled(bool),
    /// Set cava height percentage (10-80) and persist.
    SetCavaSize(u8),

    // ── Lifecycle ───────────────────────────────────────────────────────
    /// Subscribe to event broadcast. Daemon's first event after this is a
    /// fresh state snapshot, then incremental events.
    Subscribe,
    /// Request graceful daemon shutdown.
    Shutdown,
    /// Health check — daemon replies with `Pong`.
    Ping,
}

/// How an `EnqueueSongs` request integrates the new songs with the existing
/// queue.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum EnqueueMode {
    /// Replace the current queue entirely. Optionally start playback at
    /// the given index in the new queue.
    Replace { play_from: Option<usize> },
    /// Append to the existing queue without changing what's playing.
    Append,
    /// Insert after the given queue index.
    InsertAfter(usize),
}

// ────────────────────────────────────────────────────────────────────────────
// Responses: replies daemon sends back
// ────────────────────────────────────────────────────────────────────────────

/// Reply to a `DaemonRequest`. Most commands return `Ok` with no payload;
/// queries return typed payloads.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum DaemonResponse {
    /// Command succeeded with no payload.
    Ok,
    /// Command failed; the string is a human-readable error.
    Err(String),
    /// Reply to `LoadArtist`: the artist's albums, sorted by year.
    ArtistAlbums(Vec<Album>),
    /// Reply to `LoadAlbum`: the album's songs in track order.
    AlbumSongs(Vec<Child>),
    /// Reply to `LoadPlaylist`: the playlist's songs in playlist order.
    PlaylistSongs(Vec<Child>),
    /// Reply to `TestServerConnection`: whether the connection succeeded
    /// and a human-readable message.
    ConnectionTestResult { ok: bool, message: String },
    /// Reply to `Ping`.
    Pong,
}

// ────────────────────────────────────────────────────────────────────────────
// Events: server-pushed state changes
// ────────────────────────────────────────────────────────────────────────────

/// State-change broadcast from daemon to all subscribed clients. The wire
/// flow is: client sends `Subscribe`, daemon replies `Ok`, daemon then pushes
/// these events whenever state changes.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum DaemonEvent {
    /// The queue contents changed (track added/removed/reordered, or
    /// `queue_position` advanced).
    QueueChanged,
    /// `now_playing` state changed (Playing/Paused/Stopped, song change,
    /// sample-rate change). Position-only updates use `PositionTick` to
    /// avoid event spam.
    NowPlayingChanged,
    /// Cheap, lossy position tick. Emitted from a `tokio::sync::watch`-style
    /// channel; subscribers expect it at ~2 Hz when playing.
    PositionTick(f64),
    /// One of the library cache slots changed.
    LibraryChanged(LibrarySection),
    /// User-facing toast/notification from the daemon.
    Notification {
        message: String,
        is_error: bool,
    },
    /// Configuration changed (theme, server, cava knobs). Clients should
    /// reload their local config view.
    ConfigChanged(Config),
    /// Daemon is shutting down. Clients should disconnect cleanly.
    Shutdown,
}

/// Which slice of the library cache changed in a `LibraryChanged` event.
/// Phase 6 splits these into finer-grained events that carry the new payload
/// inline; phase 2 keeps it coarse.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum LibrarySection {
    Starred,
    Random,
    Artists,
    Albums,
    AlbumSongs,
    Playlists,
    PlaylistSongs,
}

// ────────────────────────────────────────────────────────────────────────────
// Errors
// ────────────────────────────────────────────────────────────────────────────

/// IPC-layer error. Wraps transport, serialization, and daemon-side failures.
/// Today (in-process) only the `Daemon` variant is used; phase 4 lights up
/// `Disconnected`, `Transport`, and `Serialize`.
#[derive(Debug, thiserror::Error)]
pub enum IpcError {
    /// Daemon returned an `Err(message)` response.
    #[error("daemon error: {0}")]
    Daemon(String),

    /// Socket disconnected mid-flight. Phase 4+.
    #[error("daemon disconnected")]
    Disconnected,

    /// Transport-level IO failure on the Unix socket. Phase 4+.
    #[error("transport: {0}")]
    Transport(#[from] std::io::Error),

    /// Wire-format serialization or deserialization failure. Phase 4+.
    #[error("serialize: {0}")]
    Serialize(#[from] serde_json::Error),
}
