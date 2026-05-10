//! Ferrosonic — terminal-based Subsonic music client. Library crate
//! exposing the shared modules consumed by the two binary targets:
//!
//! - `ferrosonic` (`src/bin/ferrosonic.rs`) — the TUI client. After
//!   phase 5b connects to `ferrosonicd` via a Unix-socket
//!   [`crate::ipc::SocketClient`]; phase 5a still runs the in-process
//!   `App::new` path unchanged.
//! - `ferrosonicd` (`src/bin/ferrosonicd.rs`) — the long-lived
//!   daemon. Owns mpv + the queue + the library cache + the MPRIS
//!   server, accepts client connections via [`crate::ipc::server`].
//!
//! Module visibility:
//! - `audio`, `subsonic`, `config`, `error`, `mpris`, `ipc`, `daemon`
//!   are shared.
//! - `app`, `ui` are only consumed by the TUI binary; exposed `pub`
//!   here for now since both binaries live in the same crate.

pub mod app;
pub mod audio;
pub mod config;
pub mod daemon;
pub mod error;
pub mod ipc;
pub mod mpris;
pub mod subsonic;
pub mod ui;
