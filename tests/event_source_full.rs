//! EventSource implementations.

use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ferrosonic::app::event_source::{ChannelEventSource, CrosstermEventSource, EventSource};
use std::time::Duration;

#[tokio::test]
async fn crossterm_event_source_returns_none_on_short_timeout() {
    let mut src = CrosstermEventSource;
    let result = src.next(Duration::from_millis(20)).await;
    assert!(
        result.is_none(),
        "no real terminal events should arrive in 20ms"
    );
}

#[tokio::test]
async fn channel_event_source_delivers_sent_event() {
    let (tx, mut src) = ChannelEventSource::new();
    let mut k = KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE);
    k.kind = KeyEventKind::Press;
    tx.send(Event::Key(k)).await.unwrap();
    let evt = src.next(Duration::from_millis(100)).await;
    assert!(matches!(evt, Some(Event::Key(_))));
}

#[tokio::test]
async fn channel_event_source_returns_none_on_timeout_when_empty() {
    let (_tx, mut src) = ChannelEventSource::new();
    let evt = src.next(Duration::from_millis(30)).await;
    assert!(evt.is_none());
}

#[tokio::test]
async fn channel_event_source_returns_none_when_sender_dropped() {
    let (tx, mut src) = ChannelEventSource::new();
    drop(tx);
    let evt = src.next(Duration::from_millis(50)).await;
    assert!(evt.is_none());
}

#[tokio::test]
async fn channel_event_source_drains_multiple_events_in_order() {
    let (tx, mut src) = ChannelEventSource::new();
    let mk = |c: char| {
        let mut k = KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE);
        k.kind = KeyEventKind::Press;
        Event::Key(k)
    };
    tx.send(mk('a')).await.unwrap();
    tx.send(mk('b')).await.unwrap();
    let e1 = src.next(Duration::from_millis(50)).await.unwrap();
    let e2 = src.next(Duration::from_millis(50)).await.unwrap();
    match (e1, e2) {
        (Event::Key(k1), Event::Key(k2)) => {
            assert_eq!(k1.code, KeyCode::Char('a'));
            assert_eq!(k2.code, KeyCode::Char('b'));
        }
        _ => panic!("expected two key events"),
    }
}
