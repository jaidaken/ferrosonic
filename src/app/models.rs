use strum_macros::{Display, EnumIter};

#[derive(Display, EnumIter, Clone, Debug, PartialEq)]
pub enum SongOption {
    Starred,
    Random,
}
