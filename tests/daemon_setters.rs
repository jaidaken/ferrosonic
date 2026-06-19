//! daemon/core.rs: every set_* setting accessor and getter.

mod common;

use common::TestDaemon;
use ferrosonic::config::RepeatMode;
use serial_test::serial;

#[tokio::test]
#[serial]
async fn set_theme_persists_and_emits() {
    let td = TestDaemon::new().await;
    td.core.set_theme("MyTheme").await.unwrap();
    let s = td.state.read().await;
    assert_eq!(s.config.theme, "MyTheme");
}

#[tokio::test]
#[serial]
async fn set_cava_enabled_persists() {
    let td = TestDaemon::new().await;
    td.core.set_cava_enabled(true).await.unwrap();
    let s = td.state.read().await;
    assert!(s.config.cava);
    drop(s);
    td.core.set_cava_enabled(false).await.unwrap();
    let s = td.state.read().await;
    assert!(!s.config.cava);
}

#[tokio::test]
#[serial]
async fn set_daemon_enabled_persists() {
    let td = TestDaemon::new().await;
    td.core.set_daemon_enabled(true).await.unwrap();
    assert!(td.state.read().await.config.daemon);
    td.core.set_daemon_enabled(false).await.unwrap();
    assert!(!td.state.read().await.config.daemon);
}

#[tokio::test]
#[serial]
async fn set_auto_continue_persists() {
    let td = TestDaemon::new().await;
    td.core.set_auto_continue(true).await.unwrap();
    assert!(td.state.read().await.config.auto_continue);
    td.core.set_auto_continue(false).await.unwrap();
    assert!(!td.state.read().await.config.auto_continue);
}

#[tokio::test]
#[serial]
async fn set_notifications_persists() {
    let td = TestDaemon::new().await;
    td.core.set_notifications(false).await.unwrap();
    assert!(!td.state.read().await.config.notifications);
    td.core.set_notifications(true).await.unwrap();
    assert!(td.state.read().await.config.notifications);
}

#[tokio::test]
#[serial]
async fn set_repeat_mode_persists_and_broadcasts() {
    let td = TestDaemon::new().await;
    let mut rx = td.core.subscribe();
    td.core.set_repeat_mode(RepeatMode::All).await.unwrap();
    assert_eq!(td.state.read().await.config.repeat_mode, RepeatMode::All);
    let _ = tokio::time::timeout(std::time::Duration::from_millis(200), rx.recv()).await;
}

#[tokio::test]
#[serial]
async fn set_cover_art_enabled_persists() {
    let td = TestDaemon::new().await;
    td.core.set_cover_art_enabled(false).await.unwrap();
    assert!(!td.state.read().await.config.cover_art);
    td.core.set_cover_art_enabled(true).await.unwrap();
    assert!(td.state.read().await.config.cover_art);
}

#[tokio::test]
#[serial]
async fn set_cover_art_size_clamps_to_range() {
    let td = TestDaemon::new().await;
    td.core.set_cover_art_size(2).await.unwrap();
    assert_eq!(td.state.read().await.config.cover_art_size, 8);
    td.core.set_cover_art_size(99).await.unwrap();
    assert_eq!(td.state.read().await.config.cover_art_size, 24);
    td.core.set_cover_art_size(16).await.unwrap();
    assert_eq!(td.state.read().await.config.cover_art_size, 16);
}

#[tokio::test]
#[serial]
async fn set_cava_size_clamps_to_range() {
    let td = TestDaemon::new().await;
    td.core.set_cava_size(2).await.unwrap();
    assert_eq!(td.state.read().await.config.cava_size, 10);
    td.core.set_cava_size(255).await.unwrap();
    assert_eq!(td.state.read().await.config.cava_size, 80);
    td.core.set_cava_size(40).await.unwrap();
    assert_eq!(td.state.read().await.config.cava_size, 40);
}

#[tokio::test]
#[serial]
async fn test_server_connection_with_bad_url_returns_false() {
    let td = TestDaemon::new().await;
    let (ok, _msg) = td
        .core
        .test_server_connection("not a url", "u", &"p".into())
        .await;
    assert!(!ok);
}

#[tokio::test]
#[serial]
async fn get_cover_art_with_no_subsonic_returns_empty() {
    let td = TestDaemon::new().await;
    {
        let mut s = td.state.write().await;
        s.config.base_url.clear();
    }
    let bytes = td.core.get_cover_art("art-1", 64).await;
    assert!(bytes.is_empty());
}

#[tokio::test]
#[serial]
async fn refresh_starred_does_not_crash_with_fake_subsonic() {
    let td = TestDaemon::new().await;
    td.fake_subsonic.expect_starred_with(&["s0", "s1"]).await;
    td.core.refresh_starred().await;
    let s = td.state.read().await;
    assert_eq!(s.library.starred_songs.len(), 2);
}

#[tokio::test]
#[serial]
async fn refresh_artists_does_not_crash_with_fake_subsonic() {
    let td = TestDaemon::new().await;
    td.core.refresh_artists().await;
}

#[tokio::test]
#[serial]
async fn refresh_playlists_does_not_crash_with_fake_subsonic() {
    let td = TestDaemon::new().await;
    td.core.refresh_playlists().await;
}

#[tokio::test]
#[serial]
async fn refresh_random_does_not_crash_with_fake_subsonic() {
    let td = TestDaemon::new().await;
    td.core.refresh_random().await;
}

#[tokio::test]
#[serial]
async fn shuffle_queue_with_empty_is_safe() {
    let td = TestDaemon::new().await;
    td.core.shuffle_queue().await;
}

#[tokio::test]
#[serial]
async fn clear_queue_history_with_empty_returns_zero() {
    let td = TestDaemon::new().await;
    let removed = td.core.clear_queue_history().await;
    assert_eq!(removed, 0);
}

#[tokio::test]
#[serial]
async fn move_queue_item_with_invalid_indices_does_not_crash() {
    let td = TestDaemon::new().await;
    td.core.move_queue_item(99, 100).await;
}

#[tokio::test]
#[serial]
async fn snapshot_returns_current_state() {
    let td = TestDaemon::new().await;
    let snap = td.core.snapshot().await;
    assert!(snap.queue.is_empty());
}

#[tokio::test]
#[serial]
async fn broadcast_queue_changed_emits_event() {
    let td = TestDaemon::new().await;
    let mut rx = td.core.subscribe();
    td.core.broadcast_queue_changed().await;
    let evt = tokio::time::timeout(std::time::Duration::from_millis(500), rx.recv())
        .await
        .expect("broadcast_queue_changed must emit within 500ms")
        .expect("event channel must yield event");
    assert!(matches!(
        evt,
        ferrosonic::ipc::protocol::DaemonEvent::QueueChanged { .. }
    ));
}

#[tokio::test]
#[serial]
async fn load_artist_with_no_subsonic_does_not_crash() {
    let td = TestDaemon::new().await;
    {
        let mut s = td.state.write().await;
        s.config.base_url.clear();
    }
    td.core.load_artist("unknown-artist").await;
}
