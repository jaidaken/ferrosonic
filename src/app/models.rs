//! Small UI model enums shared across pages.

use strum_macros::{Display, EnumIter};

#[derive(Display, EnumIter, Clone, Copy, Debug, PartialEq, Eq)]
/// Which song list the Quick Play page shows.
pub enum SongOption {
    /// Starred songs list.
    Starred,
    /// Random songs list.
    Random,
    /// Songs of one randomly-picked album.
    #[strum(to_string = "Random Album")]
    RandomAlbum,
    /// Songs of the server's most recently added album.
    #[strum(to_string = "Newest Album")]
    NewestAlbum,
    /// Songs of the server's most recently played album.
    #[strum(to_string = "Recently Played")]
    RecentlyPlayed,
    /// Songs of the server's most frequently played album.
    #[strum(to_string = "Most Played")]
    MostPlayed,
    /// Songs of the server's highest-rated album.
    #[strum(to_string = "Highest Rated")]
    HighestRated,
}

impl SongOption {
    /// Short label used when the option pane needs multiple columns.
    #[must_use]
    pub const fn compact_label(self) -> &'static str {
        match self {
            Self::Starred => "Starred",
            Self::Random => "Random",
            Self::RandomAlbum => "Random Alb",
            Self::NewestAlbum => "Newest Alb",
            Self::RecentlyPlayed => "Recent Alb",
            Self::MostPlayed => "Most Played",
            Self::HighestRated => "Top Rated",
        }
    }

    /// Daemon refresh request for this option.
    #[must_use]
    pub const fn refresh_request(self) -> crate::ipc::protocol::DaemonRequest {
        use crate::ipc::protocol::{DaemonRequest, QuickPlayAlbumKind};
        match self {
            Self::Starred => DaemonRequest::RefreshStarred,
            Self::Random => DaemonRequest::RefreshRandom,
            Self::RandomAlbum => DaemonRequest::RefreshRandomAlbum,
            Self::NewestAlbum => DaemonRequest::RefreshQuickPlayAlbum(QuickPlayAlbumKind::Newest),
            Self::RecentlyPlayed => {
                DaemonRequest::RefreshQuickPlayAlbum(QuickPlayAlbumKind::Recent)
            }
            Self::MostPlayed => DaemonRequest::RefreshQuickPlayAlbum(QuickPlayAlbumKind::Frequent),
            Self::HighestRated => DaemonRequest::RefreshQuickPlayAlbum(QuickPlayAlbumKind::Highest),
        }
    }

    /// Curated album category backing this option, if it is album-list based.
    #[must_use]
    pub const fn album_kind(self) -> Option<crate::ipc::protocol::QuickPlayAlbumKind> {
        use crate::ipc::protocol::QuickPlayAlbumKind;
        match self {
            Self::NewestAlbum => Some(QuickPlayAlbumKind::Newest),
            Self::RecentlyPlayed => Some(QuickPlayAlbumKind::Recent),
            Self::MostPlayed => Some(QuickPlayAlbumKind::Frequent),
            Self::HighestRated => Some(QuickPlayAlbumKind::Highest),
            Self::Starred | Self::Random | Self::RandomAlbum => None,
        }
    }
}
