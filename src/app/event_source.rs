//! Backend-agnostic terminal event stream.

use std::time::Duration;

use async_trait::async_trait;
use crossterm::event::{self, Event};

/// Source of terminal input events; swapped out in tests.
#[async_trait]
pub trait EventSource: Send {
    /// Returns the next event within `timeout`, or `None` on timeout / error.
    async fn next(&mut self, timeout: Duration) -> Option<Event>;
}

/// Production [`EventSource`] reading real crossterm events.
pub struct CrosstermEventSource;

#[async_trait]
impl EventSource for CrosstermEventSource {
    async fn next(&mut self, timeout: Duration) -> Option<Event> {
        tokio::task::spawn_blocking(move || {
            if event::poll(timeout).ok()? {
                event::read().ok()
            } else {
                None
            }
        })
        .await
        .ok()
        .flatten()
    }
}

/// Test seam: events fed through an mpsc channel. The send half is
/// kept by the test driver; the receive half acts as an EventSource.
pub struct ChannelEventSource {
    rx: tokio::sync::mpsc::Receiver<Event>,
}

impl ChannelEventSource {
    /// Build the source plus the sender the test driver feeds events into.
    pub fn new() -> (tokio::sync::mpsc::Sender<Event>, Self) {
        let (tx, rx) = tokio::sync::mpsc::channel(64);
        (tx, Self { rx })
    }
}

#[async_trait]
impl EventSource for ChannelEventSource {
    async fn next(&mut self, timeout: Duration) -> Option<Event> {
        tokio::time::timeout(timeout, self.rx.recv())
            .await
            .ok()
            .flatten()
    }
}
