//! Terminal UI module

pub mod chafa_ext;
pub mod cover_art;
pub mod footer;
pub mod header;
pub mod layout;
pub mod pages;
pub mod styled_lines;
pub mod theme;
mod theme_builtins;
pub mod widgets;

pub use layout::draw;
