//! Daemon-side state. Holds everything the daemon owns: live audio session
//! (queue, queue position, now playing) plus the Subsonic library cache and
//! the canonical config. The TUI reads a snapshot of this state and reflects
//! it; only the daemon mutates it.

use serde::{Deserialize, Serialize};

use crate::app::state::NowPlaying;
use crate::config::Config;
use crate::daemon::library::LibraryCache;
use crate::subsonic::models::Child;

/// All state owned by the (future) ferrosonicd daemon. In phase 1 this is
/// embedded inside `AppState` alongside `ClientState` so the existing single
/// binary keeps working; phase 5 lifts it into a separate process.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct DaemonState {
    /// Application configuration (server URL, credentials, theme, cava).
    pub config: Config,
    /// Now playing information — current song, playback state, position,
    /// duration, audio properties (sample rate, bit depth, format, channels).
    pub now_playing: NowPlaying,
    /// Play queue (ordered list of songs).
    pub queue: Vec<Child>,
    /// Current position in the queue (index into `queue`).
    pub queue_position: Option<usize>,
    /// Subsonic library cache: starred/random songs, artist tree, albums,
    /// playlists, per-id song lists.
    pub library: LibraryCache,
}

impl DaemonState {
    pub fn new(config: Config) -> Self {
        Self {
            config,
            ..Default::default()
        }
    }

    /// Get the currently playing song from the queue.
    pub fn current_song(&self) -> Option<&Child> {
        self.queue_position.and_then(|pos| self.queue.get(pos))
    }
}
