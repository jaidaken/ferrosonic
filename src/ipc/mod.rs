//! IPC between TUI and ferrosonicd.

pub mod client;
pub mod frame;
pub mod path;
pub mod protocol;
pub mod server;
pub mod socket_client;
pub mod spawn;

#[allow(unused_imports)]
pub use client::{DaemonClient, InProcessClient};
#[allow(unused_imports)]
pub use protocol::{DaemonEvent, DaemonRequest, DaemonResponse, EnqueueMode, IpcError};
#[allow(unused_imports)]
pub use socket_client::SocketClient;
