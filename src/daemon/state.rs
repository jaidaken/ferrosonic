//! Daemon-owned state: queue, now-playing, library cache, config.

use serde::{Deserialize, Serialize};

use crate::config::Config;
use crate::daemon::library::LibraryCache;
use crate::subsonic::models::Child;

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct DaemonState {
    pub config: Config,
    pub now_playing: NowPlaying,
    pub queue: Vec<Child>,
    pub queue_position: Option<usize>,
    pub library: LibraryCache,
}

impl DaemonState {
    pub fn new(config: Config) -> Self {
        Self {
            config,
            ..Default::default()
        }
    }

    pub fn current_song(&self) -> Option<&Child> {
        self.queue_position.and_then(|pos| self.queue.get(pos))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum PlaybackState {
    #[default]
    Stopped,
    Playing,
    Paused,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NowPlaying {
    pub song: Option<Child>,
    pub state: PlaybackState,
    pub position: f64,
    pub duration: f64,
    pub sample_rate: Option<u32>,
    pub bit_depth: Option<u32>,
    pub format: Option<String>,
    /// "Stereo", "Mono", "5.1ch", etc.
    pub channels: Option<String>,
}

impl NowPlaying {
    pub fn progress_percent(&self) -> f64 {
        if self.duration > 0.0 {
            (self.position / self.duration).clamp(0.0, 1.0)
        } else {
            0.0
        }
    }

    pub fn format_position(&self) -> String {
        format_duration(self.position)
    }

    pub fn format_duration(&self) -> String {
        format_duration(self.duration)
    }
}

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
