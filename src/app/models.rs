//! Small UI model enums shared across pages.

use strum_macros::{Display, EnumIter};

#[derive(Display, EnumIter, Clone, Debug, PartialEq, Eq)]
/// Which song list the Quick Play page shows.
pub enum SongOption {
    /// Starred songs list.
    Starred,
    /// Random songs list.
    Random,
    /// Internet radio stations configured on the server.
    Radio,
}

impl SongOption {
    /// The option below this one in the selector, if any.
    #[must_use]
    pub const fn next(&self) -> Option<Self> {
        match self {
            Self::Starred => Some(Self::Random),
            Self::Random => Some(Self::Radio),
            Self::Radio => None,
        }
    }

    /// The option above this one in the selector, if any.
    #[must_use]
    pub const fn prev(&self) -> Option<Self> {
        match self {
            Self::Starred => None,
            Self::Random => Some(Self::Starred),
            Self::Radio => Some(Self::Random),
        }
    }

    /// Zero-based row of this option in the selector list.
    #[must_use]
    pub const fn index(&self) -> usize {
        match self {
            Self::Starred => 0,
            Self::Random => 1,
            Self::Radio => 2,
        }
    }

    /// Option at selector row `row`, if any.
    #[must_use]
    pub const fn from_index(row: usize) -> Option<Self> {
        match row {
            0 => Some(Self::Starred),
            1 => Some(Self::Random),
            2 => Some(Self::Radio),
            _ => None,
        }
    }

    /// Daemon request that (re)loads the list this option shows.
    #[must_use]
    pub const fn refresh_request(&self) -> crate::ipc::protocol::DaemonRequest {
        use crate::ipc::protocol::DaemonRequest;
        match self {
            Self::Starred => DaemonRequest::RefreshStarred,
            Self::Random => DaemonRequest::RefreshRandom,
            Self::Radio => DaemonRequest::RefreshRadioStations,
        }
    }
}
