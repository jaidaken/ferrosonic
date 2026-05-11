//! Ferrosonic, terminal-based Subsonic music client.

// `drop(guard)` on a MutexGuard / RwLockWriteGuard releases the lock
// before the next .await. Clippy doesn't see that as the load-bearing
// effect it is.
#![allow(clippy::drop_non_drop)]

pub mod app;
pub mod audio;
pub mod config;
pub mod daemon;
pub mod error;
pub mod ipc;
pub mod mpris;
pub mod subsonic;
pub mod ui;
