//! Small UI model enums shared across pages.

use strum_macros::{Display, EnumIter};

#[derive(Display, EnumIter, Clone, Debug, PartialEq, Eq)]
/// Which song list the Quick Play page shows.
pub enum SongOption {
    /// Starred songs list.
    Starred,
    /// Random songs list.
    Random,
    /// Songs of one randomly-picked album.
    #[strum(to_string = "Random Album")]
    RandomAlbum,
}
