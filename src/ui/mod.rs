//! Terminal UI module

pub mod chafa_ext;
pub mod cover_art;
pub mod footer;
pub mod header;
pub mod layout;
pub mod pages;
pub mod quit_prompt;
pub mod styled_lines;
pub mod theme;
mod theme_builtins;
pub mod widget_cava;
pub mod widget_now_playing;

pub use layout::draw;
pub use widget_cava::CavaWidget;
pub use widget_now_playing::NowPlayingWidget;
