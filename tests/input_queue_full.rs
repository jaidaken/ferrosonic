//! Exhaustive input_queue.rs branches.

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

fn key_mod(code: KeyCode, mods: KeyModifiers) -> KeyEvent {
    let mut k = KeyEvent::new(code, mods);
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
        artist_id: None,
        album_id: None,
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

async fn build_app() -> AppFixture {
    let tempdir = common::tempdir();
    std::env::set_var("FERROSONIC_CONFIG_DIR", tempdir.path());
    let mut config = Config::new();
    config.daemon = false;
    let mut app = App::new(config);
    app.handle_key(key(KeyCode::F(2))).await.unwrap();
    AppFixture {
        app,
        _tempdir: tempdir,
    }
}

#[tokio::test]
#[serial]
async fn up_with_no_selection_initializes_to_zero_when_queue_nonempty() {
    let mut fx = build_app().await;
    {
        let mut ds = fx.app.daemon_state.write().await;
        ds.queue = vec![song("a"), song("b")];
    }
    fx.app.handle_key(key(KeyCode::Up)).await.unwrap();
    assert_eq!(
        fx.app.client_state.read().await.queue_state.selected,
        Some(0)
    );
}

#[tokio::test]
#[serial]
async fn up_at_top_stays_at_zero() {
    let mut fx = build_app().await;
    {
        let mut ds = fx.app.daemon_state.write().await;
        ds.queue = vec![song("a")];
    }
    {
        let mut cs = fx.app.client_state.write().await;
        cs.queue_state.selected = Some(0);
    }
    fx.app.handle_key(key(KeyCode::Up)).await.unwrap();
    assert_eq!(
        fx.app.client_state.read().await.queue_state.selected,
        Some(0)
    );
}

#[tokio::test]
#[serial]
async fn down_past_end_stays_at_max() {
    let mut fx = build_app().await;
    {
        let mut ds = fx.app.daemon_state.write().await;
        ds.queue = vec![song("a"), song("b")];
    }
    {
        let mut cs = fx.app.client_state.write().await;
        cs.queue_state.selected = Some(1);
    }
    fx.app.handle_key(key(KeyCode::Down)).await.unwrap();
    assert_eq!(
        fx.app.client_state.read().await.queue_state.selected,
        Some(1)
    );
}

#[tokio::test]
#[serial]
async fn j_acts_as_down() {
    let mut fx = build_app().await;
    {
        let mut ds = fx.app.daemon_state.write().await;
        ds.queue = vec![song("a"), song("b")];
    }
    fx.app.handle_key(key(KeyCode::Char('j'))).await.unwrap();
    assert_eq!(
        fx.app.client_state.read().await.queue_state.selected,
        Some(0)
    );
}

#[tokio::test]
#[serial]
async fn k_acts_as_up() {
    let mut fx = build_app().await;
    {
        let mut ds = fx.app.daemon_state.write().await;
        ds.queue = vec![song("a"), song("b")];
    }
    {
        let mut cs = fx.app.client_state.write().await;
        cs.queue_state.selected = Some(1);
    }
    fx.app.handle_key(key(KeyCode::Char('k'))).await.unwrap();
    assert_eq!(
        fx.app.client_state.read().await.queue_state.selected,
        Some(0)
    );
}

#[tokio::test]
#[serial]
async fn enter_with_valid_selection_plays_index() {
    let mut fx = build_app().await;
    {
        let mut ds = fx.app.daemon_state.write().await;
        ds.queue = vec![song("a"), song("b"), song("c")];
    }
    {
        let mut cs = fx.app.client_state.write().await;
        cs.queue_state.selected = Some(1);
    }
    let _ = fx.app.handle_key(key(KeyCode::Enter)).await;
    assert_eq!(
        fx.app.client_state.read().await.queue_state.selected,
        Some(1)
    );
    assert_eq!(fx.app.daemon_state.read().await.queue.len(), 3);
}

#[tokio::test]
#[serial]
async fn enter_with_no_selection_is_noop() {
    let mut fx = build_app().await;
    {
        let mut ds = fx.app.daemon_state.write().await;
        ds.queue = vec![song("a")];
    }
    fx.app.handle_key(key(KeyCode::Enter)).await.unwrap();
    assert!(fx
        .app
        .client_state
        .read()
        .await
        .queue_state
        .selected
        .is_none());
    assert!(fx.app.daemon_state.read().await.queue_position.is_none());
}

#[tokio::test]
#[serial]
async fn d_removes_selected_song() {
    let mut fx = build_app().await;
    {
        let mut ds = fx.app.daemon_state.write().await;
        ds.queue = vec![song("keep0"), song("remove"), song("keep2")];
    }
    {
        let mut cs = fx.app.client_state.write().await;
        cs.queue_state.selected = Some(1);
    }
    fx.app.handle_key(key(KeyCode::Char('d'))).await.unwrap();
    let ds = fx.app.daemon_state.read().await;
    assert_eq!(ds.queue.len(), 2);
    assert!(!ds.queue.iter().any(|s| s.id == "remove"));
}

#[tokio::test]
#[serial]
async fn d_at_last_index_moves_selection_to_new_end() {
    let mut fx = build_app().await;
    {
        let mut ds = fx.app.daemon_state.write().await;
        ds.queue = vec![song("a"), song("b"), song("c")];
    }
    {
        let mut cs = fx.app.client_state.write().await;
        cs.queue_state.selected = Some(2);
    }
    fx.app.handle_key(key(KeyCode::Char('d'))).await.unwrap();
    assert_eq!(
        fx.app.client_state.read().await.queue_state.selected,
        Some(1)
    );
}

#[tokio::test]
#[serial]
async fn d_with_single_item_clears_selection() {
    let mut fx = build_app().await;
    {
        let mut ds = fx.app.daemon_state.write().await;
        ds.queue = vec![song("only")];
    }
    {
        let mut cs = fx.app.client_state.write().await;
        cs.queue_state.selected = Some(0);
    }
    fx.app.handle_key(key(KeyCode::Char('d'))).await.unwrap();
    assert!(fx
        .app
        .client_state
        .read()
        .await
        .queue_state
        .selected
        .is_none());
}

#[tokio::test]
#[serial]
async fn d_with_no_selection_is_noop() {
    let mut fx = build_app().await;
    {
        let mut ds = fx.app.daemon_state.write().await;
        ds.queue = vec![song("a")];
    }
    fx.app.handle_key(key(KeyCode::Char('d'))).await.unwrap();
    let ds = fx.app.daemon_state.read().await;
    assert_eq!(ds.queue.len(), 1);
    assert_eq!(ds.queue[0].id, "a");
    assert!(fx.app.client_state.read().await.notification.is_none());
}

#[tokio::test]
#[serial]
async fn capital_j_moves_song_down_in_queue() {
    let mut fx = build_app().await;
    {
        let mut ds = fx.app.daemon_state.write().await;
        ds.queue = vec![song("a"), song("b"), song("c")];
    }
    {
        let mut cs = fx.app.client_state.write().await;
        cs.queue_state.selected = Some(0);
    }
    fx.app
        .handle_key(key_mod(KeyCode::Char('J'), KeyModifiers::SHIFT))
        .await
        .unwrap();
    assert_eq!(
        fx.app.client_state.read().await.queue_state.selected,
        Some(1)
    );
}

#[tokio::test]
#[serial]
async fn capital_j_at_end_is_noop() {
    let mut fx = build_app().await;
    {
        let mut ds = fx.app.daemon_state.write().await;
        ds.queue = vec![song("a"), song("b")];
    }
    {
        let mut cs = fx.app.client_state.write().await;
        cs.queue_state.selected = Some(1);
    }
    fx.app
        .handle_key(key_mod(KeyCode::Char('J'), KeyModifiers::SHIFT))
        .await
        .unwrap();
    assert_eq!(
        fx.app.client_state.read().await.queue_state.selected,
        Some(1)
    );
    let ds = fx.app.daemon_state.read().await;
    assert_eq!(ds.queue[0].id, "a");
    assert_eq!(ds.queue[1].id, "b");
}

#[tokio::test]
#[serial]
async fn capital_k_moves_song_up_in_queue() {
    let mut fx = build_app().await;
    {
        let mut ds = fx.app.daemon_state.write().await;
        ds.queue = vec![song("a"), song("b"), song("c")];
    }
    {
        let mut cs = fx.app.client_state.write().await;
        cs.queue_state.selected = Some(2);
    }
    fx.app
        .handle_key(key_mod(KeyCode::Char('K'), KeyModifiers::SHIFT))
        .await
        .unwrap();
    assert_eq!(
        fx.app.client_state.read().await.queue_state.selected,
        Some(1)
    );
}

#[tokio::test]
#[serial]
async fn capital_k_at_top_is_noop() {
    let mut fx = build_app().await;
    {
        let mut ds = fx.app.daemon_state.write().await;
        ds.queue = vec![song("a"), song("b")];
    }
    {
        let mut cs = fx.app.client_state.write().await;
        cs.queue_state.selected = Some(0);
    }
    fx.app
        .handle_key(key_mod(KeyCode::Char('K'), KeyModifiers::SHIFT))
        .await
        .unwrap();
    assert_eq!(
        fx.app.client_state.read().await.queue_state.selected,
        Some(0)
    );
    let ds = fx.app.daemon_state.read().await;
    assert_eq!(ds.queue[0].id, "a");
    assert_eq!(ds.queue[1].id, "b");
}

#[tokio::test]
#[serial]
async fn t_shuffles_queue() {
    let mut fx = build_app().await;
    {
        let mut ds = fx.app.daemon_state.write().await;
        ds.queue = vec![song("a"), song("b")];
    }
    fx.app.handle_key(key(KeyCode::Char('t'))).await.unwrap();
    let notif = fx.app.client_state.read().await.notification.clone();
    let msg = notif.expect("shuffle should set a notification").message;
    assert_eq!(msg, "Queue shuffled");
}

#[tokio::test]
#[serial]
async fn c_with_no_history_notifies_no_history_to_clear() {
    let mut fx = build_app().await;
    {
        let mut ds = fx.app.daemon_state.write().await;
        ds.queue = vec![song("a")];
        ds.queue_position = Some(0);
    }
    fx.app.handle_key(key(KeyCode::Char('c'))).await.unwrap();
    let notif = fx.app.client_state.read().await.notification.clone();
    let msg = notif
        .expect("c with no history sets a notification")
        .message;
    assert_eq!(msg, "No history to clear");
}

#[tokio::test]
#[serial]
async fn m_with_selection_stars_song() {
    let mut fx = build_app().await;
    {
        let mut ds = fx.app.daemon_state.write().await;
        ds.queue = vec![song("starme")];
    }
    {
        let mut cs = fx.app.client_state.write().await;
        cs.queue_state.selected = Some(0);
    }
    fx.app.handle_key(key(KeyCode::Char('m'))).await.unwrap();
    let ds = fx.app.daemon_state.read().await;
    assert_eq!(ds.queue.len(), 1);
    assert_eq!(ds.queue[0].id, "starme");
    assert_eq!(
        fx.app.client_state.read().await.queue_state.selected,
        Some(0)
    );
}

#[tokio::test]
#[serial]
async fn m_with_no_selection_is_noop() {
    let mut fx = build_app().await;
    fx.app.handle_key(key(KeyCode::Char('m'))).await.unwrap();
    let cs = fx.app.client_state.read().await;
    assert!(cs.queue_state.selected.is_none());
    assert!(cs.notification.is_none());
}

#[tokio::test]
#[serial]
async fn unhandled_key_is_noop() {
    let mut fx = build_app().await;
    {
        let mut ds = fx.app.daemon_state.write().await;
        ds.queue = vec![song("a"), song("b")];
    }
    fx.app.handle_key(key(KeyCode::Insert)).await.unwrap();
    let ds = fx.app.daemon_state.read().await;
    assert_eq!(ds.queue.len(), 2);
    assert_eq!(ds.queue[0].id, "a");
    let cs = fx.app.client_state.read().await;
    assert!(cs.queue_state.selected.is_none());
    assert!(cs.notification.is_none());
}
