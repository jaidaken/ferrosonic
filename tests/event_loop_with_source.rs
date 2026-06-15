//! End-to-end event loop driven by ChannelEventSource + TestBackend.

mod common;
use std::time::Duration;

use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ferrosonic::app::event_source::ChannelEventSource;
use ferrosonic::app::state::Page;
use ferrosonic::app::App;
use ferrosonic::config::Config;
use ratatui::backend::TestBackend;
use ratatui::Terminal;
use serial_test::serial;

fn key_event(code: KeyCode) -> Event {
    let mut k = KeyEvent::new(code, KeyModifiers::NONE);
    k.kind = KeyEventKind::Press;
    Event::Key(k)
}

struct AppFixture {
    app: App,
    _tempdir: tempfile::TempDir,
}

async fn build_app() -> AppFixture {
    let tempdir = common::tempdir();
    std::env::set_var("FERROSONIC_CONFIG_DIR", tempdir.path());
    let mut config = Config::new();
    config.daemon = false;
    let app = App::new(config);
    AppFixture {
        app,
        _tempdir: tempdir,
    }
}

#[tokio::test]
#[serial]
async fn loop_terminates_when_q_event_arrives() {
    let mut fx = build_app().await;
    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).unwrap();
    let (tx, mut source) = ChannelEventSource::new();
    tx.send(key_event(KeyCode::Char('q'))).await.unwrap();

    tokio::time::timeout(
        Duration::from_secs(2),
        fx.app.run_with_source(&mut terminal, &mut source),
    )
    .await
    .expect("loop should terminate quickly after q")
    .expect("loop returns Ok");

    assert!(fx.app.should_quit().await);
}

#[tokio::test]
#[serial]
async fn loop_processes_f_key_before_quit() {
    let mut fx = build_app().await;
    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).unwrap();
    let (tx, mut source) = ChannelEventSource::new();
    tx.send(key_event(KeyCode::F(2))).await.unwrap();
    tx.send(key_event(KeyCode::Char('q'))).await.unwrap();

    tokio::time::timeout(
        Duration::from_secs(2),
        fx.app.run_with_source(&mut terminal, &mut source),
    )
    .await
    .expect("loop should terminate")
    .unwrap();

    let cs = fx.app.client_state.read().await;
    assert_eq!(
        cs.page,
        Page::Queue,
        "F2 must switch to Queue before q quits"
    );
}

#[tokio::test]
#[serial]
async fn loop_handles_multiple_navigation_events() {
    let mut fx = build_app().await;
    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).unwrap();
    let (tx, mut source) = ChannelEventSource::new();
    tx.send(key_event(KeyCode::F(6))).await.unwrap();
    tx.send(key_event(KeyCode::Down)).await.unwrap();
    tx.send(key_event(KeyCode::Down)).await.unwrap();
    tx.send(key_event(KeyCode::Char('q'))).await.unwrap();

    tokio::time::timeout(
        Duration::from_secs(2),
        fx.app.run_with_source(&mut terminal, &mut source),
    )
    .await
    .expect("loop should terminate")
    .unwrap();

    let cs = fx.app.client_state.read().await;
    assert_eq!(cs.page, Page::Settings);
    assert!(cs.settings_state.selected_field >= 2);
}

#[tokio::test]
#[serial]
async fn loop_redraws_each_tick() {
    let mut fx = build_app().await;
    let backend = TestBackend::new(40, 10);
    let mut terminal = Terminal::new(backend).unwrap();
    let (tx, mut source) = ChannelEventSource::new();
    tx.send(key_event(KeyCode::Char('q'))).await.unwrap();

    fx.app
        .run_with_source(&mut terminal, &mut source)
        .await
        .unwrap();

    let buf = terminal.backend().buffer();
    let total_cells = buf.area.width as usize * buf.area.height as usize;
    let non_empty = (0..buf.area.height)
        .flat_map(|y| (0..buf.area.width).map(move |x| (x, y)))
        .filter(|(x, y)| buf[(*x, *y)].symbol() != " ")
        .count();
    assert!(
        non_empty > 0,
        "loop must render something into the buffer; got {} of {} non-empty",
        non_empty,
        total_cells
    );
}

#[tokio::test]
#[serial]
async fn loop_returns_ok_when_quit_via_state_flag() {
    let mut fx = build_app().await;
    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).unwrap();
    let (_tx, mut source) = ChannelEventSource::new();
    {
        let mut cs = fx.app.client_state.write().await;
        cs.should_quit = true;
    }
    fx.app
        .run_with_source(&mut terminal, &mut source)
        .await
        .unwrap();
    assert!(fx.app.should_quit().await);
}
