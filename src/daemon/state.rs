//! Daemon-owned state: queue, now-playing, library cache, config.

use serde::{Deserialize, Serialize};

use crate::app::state::NowPlaying;
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
