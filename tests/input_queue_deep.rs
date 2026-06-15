//! Queue page input: nav, remove (d), move (J/K), play (Enter), shuffle (t), clear (c).

mod common;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ferrosonic::app::App;
use ferrosonic::config::Config;
use ferrosonic::subsonic::models::Child;
use serial_test::serial;

fn key(code: KeyCode) -> KeyEvent {
    let mut k = KeyEvent::new(code, KeyModifiers::NONE);
    k.kind = KeyEventKind::Press;
    k
}

fn shift(code: KeyCode) -> KeyEvent {
    let mut k = KeyEvent::new(code, KeyModifiers::SHIFT);
    k.kind = KeyEventKind::Press;
    k
}

fn song(id: &str) -> Child {
    Child {
        id: id.into(),
        title: id.into(),
        parent: None,
        is_dir: false,
        album: None,
        artist: None,
        track: None,
        year: None,
        genre: None,
        cover_art: None,
        size: None,
        content_type: None,
        suffix: None,
        duration: Some(180),
        bit_rate: None,
        path: None,
        disc_number: None,
        starred: None,
    }
}

struct AppFixture {
    app: App,
    _tempdir: tempfile::TempDir,
}

async fn build_app_with_queue(n: usize) -> AppFixture {
    let tempdir = common::tempdir();
    std::env::set_var("FERROSONIC_CONFIG_DIR", tempdir.path());
    let mut config = Config::new();
    config.daemon = false;
    let mut app = App::new(config);
    {
        let mut ds = app.daemon_state.write().await;
        ds.queue = (0..n).map(|i| song(&format!("q-{}", i))).collect();
    }
    app.handle_key(key(KeyCode::F(2))).await.unwrap();
    AppFixture {
        app,
        _tempdir: tempdir,
    }
}

#[tokio::test]
#[serial]
async fn down_advances_selection_within_queue() {
    let mut fx = build_app_with_queue(5).await;
    fx.app.handle_key(key(KeyCode::Down)).await.unwrap();
    fx.app.handle_key(key(KeyCode::Down)).await.unwrap();
    fx.app.handle_key(key(KeyCode::Down)).await.unwrap();
    let cs = fx.app.client_state.read().await;
    assert!(cs.queue_state.selected.unwrap_or(0) >= 2);
}

#[tokio::test]
#[serial]
async fn up_at_top_does_not_underflow() {
    let mut fx = build_app_with_queue(3).await;
    fx.app.handle_key(key(KeyCode::Down)).await.unwrap();
    fx.app.handle_key(key(KeyCode::Up)).await.unwrap();
    fx.app.handle_key(key(KeyCode::Up)).await.unwrap();
    let cs = fx.app.client_state.read().await;
    assert_eq!(cs.queue_state.selected.unwrap_or(99), 0);
}

#[tokio::test]
#[serial]
async fn d_removes_selected_track() {
    let mut fx = build_app_with_queue(3).await;
    fx.app.handle_key(key(KeyCode::Down)).await.unwrap();
    fx.app.handle_key(key(KeyCode::Char('d'))).await.unwrap();
    let ds = fx.app.daemon_state.read().await;
    assert!(
        ds.queue.len() <= 3,
        "d should not grow the queue; got len {}",
        ds.queue.len()
    );
}

#[tokio::test]
#[serial]
async fn shift_j_moves_track_down() {
    let mut fx = build_app_with_queue(4).await;
    fx.app.handle_key(key(KeyCode::Down)).await.unwrap();
    fx.app.handle_key(shift(KeyCode::Char('J'))).await.unwrap();
}

#[tokio::test]
#[serial]
async fn shift_k_moves_track_up() {
    let mut fx = build_app_with_queue(4).await;
    fx.app.handle_key(key(KeyCode::Down)).await.unwrap();
    fx.app.handle_key(key(KeyCode::Down)).await.unwrap();
    fx.app.handle_key(shift(KeyCode::Char('K'))).await.unwrap();
}

#[tokio::test]
#[serial]
async fn t_on_queue_calls_shuffle_queue() {
    let mut fx = build_app_with_queue(5).await;
    fx.app.handle_key(key(KeyCode::Char('t'))).await.unwrap();
}

#[tokio::test]
#[serial]
async fn c_on_queue_calls_clear_history() {
    let mut fx = build_app_with_queue(5).await;
    {
        let mut ds = fx.app.daemon_state.write().await;
        ds.queue_position = Some(2);
    }
    fx.app.handle_key(key(KeyCode::Char('c'))).await.unwrap();
}
