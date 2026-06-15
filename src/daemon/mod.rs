//! Daemon: core, ops modules, state, persistence, polling.

pub mod core;
pub mod library;
pub mod library_ops;
pub mod loaders;
pub mod persistence;
pub mod playback_ops;
pub mod playback_tick;
pub mod polling;
pub mod queue_ops;
pub mod run;
pub mod settings_ops;
pub mod state;

pub use core::DaemonCore;
pub use library::LibraryCache;
pub use run::run;
pub use state::DaemonState;
