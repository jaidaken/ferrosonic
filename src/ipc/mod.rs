//! IPC layer between TUI client and (eventual) daemon process.
//!
//! Phase 2 of the daemon split introduces this module. Today everything runs
//! in a single process — `InProcessClient` (defined later) implements
//! `DaemonClient` by calling directly into `DaemonCore`. Phase 4 adds a
//! `SocketClient`/`SocketServer` pair so the same protocol crosses a Unix
//! socket without the rest of the codebase changing.
//!
//! `protocol.rs` defines the wire schema:
//! - `DaemonRequest` — commands the client sends to the daemon
//! - `DaemonResponse` — replies the daemon sends back
//! - `DaemonEvent` — push events the daemon broadcasts to all subscribed clients
//! - `IpcError` — error type for IPC failures (transport, serialization, daemon-side)

pub mod client;
pub mod frame;
pub mod path;
pub mod protocol;
pub mod server;
pub mod socket_client;
pub mod spawn;

#[allow(unused_imports)] // re-exported for future binary split / consumers
pub use client::{DaemonClient, InProcessClient};
#[allow(unused_imports)]
pub use protocol::{DaemonEvent, DaemonRequest, DaemonResponse, EnqueueMode, IpcError};
#[allow(unused_imports)] // wired up in phase 5 binary
pub use socket_client::SocketClient;
