//! Ferrosonic, terminal-based Subsonic music client.

// drop(guard) releases a lock before .await; clippy can't see that.

pub mod app;
pub mod audio;
pub mod config;
pub mod daemon;
pub mod error;
pub mod ipc;
pub mod mpris;
pub mod secret;
pub mod subsonic;
pub mod ui;
