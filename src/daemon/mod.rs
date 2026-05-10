//! Daemon-side state and (eventually) the daemon binary's behaviour.
//!
//! Phase 1 of the daemon split: this module currently exposes only the data
//! types that the future `ferrosonicd` will own — `DaemonState` and
//! `LibraryCache`. The daemon process itself doesn't exist yet; for now the
//! `App` owns a `DaemonState` instance directly. Subsequent phases lift the
//! state into a separate process, with the client connecting via Unix socket.

pub mod library;
pub mod state;

#[allow(unused_imports)] // re-export for future ferrosonicd binary
pub use library::LibraryCache;
pub use state::DaemonState;
