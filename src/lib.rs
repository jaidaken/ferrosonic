//! Ferrosonic, terminal-based Subsonic music client.
#![warn(clippy::pedantic, clippy::nursery, missing_docs, rust_2018_idioms)]

pub mod app;
pub mod audio;
pub mod config;
pub mod daemon;
pub mod error;
pub mod ipc;
pub mod io_util;
pub mod mpris;
pub mod proc_util;
pub mod secret;
pub mod subsonic;
pub mod ui;
