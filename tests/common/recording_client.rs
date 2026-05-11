//! `DaemonClient` stub that records every request for test assertions.

use std::sync::Arc;

use async_trait::async_trait;
use ferrosonic::ipc::client::DaemonClient;
use ferrosonic::ipc::protocol::{DaemonEvent, DaemonRequest, DaemonResponse, IpcError};
use tokio::sync::{broadcast, Mutex};

pub struct RecordingClient {
    pub recorded: Arc<Mutex<Vec<DaemonRequest>>>,
    event_tx: broadcast::Sender<DaemonEvent>,
}

impl RecordingClient {
    pub fn new() -> Arc<Self> {
        let (event_tx, _) = broadcast::channel(16);
        Arc::new(Self {
            recorded: Arc::new(Mutex::new(Vec::new())),
            event_tx,
        })
    }

    pub async fn requests(&self) -> Vec<DaemonRequest> {
        self.recorded.lock().await.clone()
    }
}

#[async_trait]
impl DaemonClient for RecordingClient {
    async fn request(&self, req: DaemonRequest) -> Result<DaemonResponse, IpcError> {
        self.recorded.lock().await.push(req);
        Ok(DaemonResponse::Ok)
    }

    fn subscribe(&self) -> broadcast::Receiver<DaemonEvent> {
        self.event_tx.subscribe()
    }
}
