//! Daemon-owned state: queue, now-playing, library cache, config.

use serde::{Deserialize, Serialize};

use crate::config::Config;
use crate::daemon::library::LibraryCache;
use crate::subsonic::models::Child;

/// Complete daemon-side state, snapshotted to clients over IPC.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct DaemonState {
    /// Persisted configuration.
    pub config: Config,
    /// Current track and playback status.
    pub now_playing: NowPlaying,
    /// Play queue, history included.
    pub queue: Vec<Child>,
    /// Index of the playing entry in `queue`, if any.
    pub queue_position: Option<usize>,
    /// Cached library data fetched from the server.
    pub library: LibraryCache,
}

impl DaemonState {
    /// Fresh state carrying `config`, everything else default.
    pub fn new(config: Config) -> Self {
        Self {
            config,
            ..Default::default()
        }
    }

    /// Song at the current queue position, if any.
    pub fn current_song(&self) -> Option<&Child> {
        self.queue_position.and_then(|pos| self.queue.get(pos))
    }
}

/// Coarse playback status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum PlaybackState {
    /// Nothing loaded.
    #[default]
    Stopped,
    /// Audio is playing.
    Playing,
    /// A track is loaded but paused.
    Paused,
}

/// Current track plus live playback properties.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NowPlaying {
    /// Loaded song, if any.
    pub song: Option<Child>,
    /// Playing / paused / stopped.
    pub state: PlaybackState,
    /// Playback position in seconds.
    pub position: f64,
    /// Track duration in seconds.
    pub duration: f64,
    /// Decoded sample rate in Hz.
    pub sample_rate: Option<u32>,
    /// Decoded bit depth.
    pub bit_depth: Option<u32>,
    /// Raw mpv audio format string.
    pub format: Option<String>,
    /// "Stereo", "Mono", "5.1ch", etc.
    pub channels: Option<String>,
}

impl NowPlaying {
    /// Position as a fraction of duration, clamped to `0.0..=1.0`. Returns `0.0` when duration is non-positive so callers never divide-by-zero or render negative progress.
    ///
    /// ```
    /// use ferrosonic::daemon::state::NowPlaying;
    /// let mut np = NowPlaying::default();
    /// np.duration = 100.0;
    /// np.position = 25.0;
    /// assert!((np.progress_percent() - 0.25).abs() < 1e-9);
    /// np.duration = 0.0;
    /// assert_eq!(np.progress_percent(), 0.0);
    /// ```
    pub fn progress_percent(&self) -> f64 {
        if self.duration > 0.0 {
            (self.position / self.duration).clamp(0.0, 1.0)
        } else {
            0.0
        }
    }

    /// Current position formatted `MM:SS` (or `HH:MM:SS` if at least one hour).
    ///
    /// ```
    /// use ferrosonic::daemon::state::NowPlaying;
    /// let mut np = NowPlaying::default();
    /// np.position = 65.0;
    /// assert_eq!(np.format_position(), "01:05");
    /// ```
    pub fn format_position(&self) -> String {
        format_duration(self.position)
    }

    /// Track duration formatted `MM:SS` (or `HH:MM:SS` if at least one hour).
    ///
    /// ```
    /// use ferrosonic::daemon::state::NowPlaying;
    /// let mut np = NowPlaying::default();
    /// np.duration = 3665.0;
    /// assert_eq!(np.format_duration(), "01:01:05");
    /// ```
    pub fn format_duration(&self) -> String {
        format_duration(self.duration)
    }
}

/// Format `seconds` as `MM:SS` under one hour, `HH:MM:SS` at one hour or above. Fractional seconds are truncated.
///
/// ```
/// use ferrosonic::daemon::state::format_duration;
/// assert_eq!(format_duration(59.9), "00:59");
/// assert_eq!(format_duration(125.0), "02:05");
/// assert_eq!(format_duration(3600.0), "01:00:00");
/// ```
pub fn format_duration(seconds: f64) -> String {
    let total_secs = seconds as u64;
    let hours = total_secs / 3600;
    let mins = (total_secs % 3600) / 60;
    let secs = total_secs % 60;

    if hours > 0 {
        format!("{:02}:{:02}:{:02}", hours, mins, secs)
    } else {
        format!("{:02}:{:02}", mins, secs)
    }
}
