//! Ferrosonic, terminal-based Subsonic music client.
#![warn(clippy::pedantic, clippy::nursery, missing_docs, rust_2018_idioms)]
// Audited: flagged sites are clear `match Some/None`, not map_or targets.
#![allow(clippy::option_if_let_else)]

pub mod app;
pub mod audio;
pub mod config;
pub mod daemon;
pub mod error;
pub mod io_util;
pub mod ipc;
pub mod mpris;
pub mod proc_util;
pub mod secret;
pub mod secret_store;
pub mod subsonic;
pub mod ui;
