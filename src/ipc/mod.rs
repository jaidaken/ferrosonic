//! IPC between TUI and ferrosonicd.

pub mod client;
pub mod frame;
pub mod path;
pub mod protocol;
pub mod server;
pub mod socket_client;
pub mod spawn;

pub use client::{DaemonClient, InProcessClient};
pub use protocol::{DaemonEvent, DaemonRequest, DaemonResponse, EnqueueMode, IpcError};
pub use socket_client::SocketClient;
