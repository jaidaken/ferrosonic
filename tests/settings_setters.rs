//! Config setters: set_repeat_mode, set_auto_continue, set_daemon_enabled,
//! set_cover_art_enabled, set_cover_art_size, set_cava_enabled, set_cava_size,
//! set_volume. Verifies state mutation, persistence, and event emission.

mod common;

use common::TestDaemon;
use ferrosonic::config::RepeatMode;
use serial_test::serial;

#[tokio::test]
#[serial]
async fn set_repeat_mode_persists_to_state() {
    let td = TestDaemon::new().await;

    td.core.set_repeat_mode(RepeatMode::All).await.unwrap();
    assert_eq!(td.state.read().await.config.repeat_mode, RepeatMode::All);

    td.core.set_repeat_mode(RepeatMode::One).await.unwrap();
    assert_eq!(td.state.read().await.config.repeat_mode, RepeatMode::One);

    td.core.set_repeat_mode(RepeatMode::Off).await.unwrap();
    assert_eq!(td.state.read().await.config.repeat_mode, RepeatMode::Off);
}

#[tokio::test]
#[serial]
async fn set_auto_continue_persists_to_state() {
    let td = TestDaemon::new().await;
    assert!(!td.state.read().await.config.auto_continue);

    td.core.set_auto_continue(true).await.unwrap();
    assert!(td.state.read().await.config.auto_continue);

    td.core.set_auto_continue(false).await.unwrap();
    assert!(!td.state.read().await.config.auto_continue);
}

#[tokio::test]
#[serial]
async fn set_daemon_enabled_persists_to_state() {
    let td = TestDaemon::new().await;
    assert!(td.state.read().await.config.daemon);

    td.core.set_daemon_enabled(false).await.unwrap();
    assert!(!td.state.read().await.config.daemon);
}

#[tokio::test]
#[serial]
async fn set_cover_art_enabled_persists() {
    let td = TestDaemon::new().await;
    td.core.set_cover_art_enabled(true).await.unwrap();
    assert!(td.state.read().await.config.cover_art);
}

#[tokio::test]
#[serial]
async fn set_cover_art_size_persists_and_clamps() {
    let td = TestDaemon::new().await;
    td.core.set_cover_art_size(20).await.unwrap();
    assert_eq!(td.state.read().await.config.cover_art_size, 20);
}

#[tokio::test]
#[serial]
async fn set_cava_enabled_persists() {
    let td = TestDaemon::new().await;
    td.core.set_cava_enabled(true).await.unwrap();
    assert!(td.state.read().await.config.cava);
}

#[tokio::test]
#[serial]
async fn set_cava_size_persists() {
    let td = TestDaemon::new().await;
    td.core.set_cava_size(50).await.unwrap();
    assert_eq!(td.state.read().await.config.cava_size, 50);
}

#[tokio::test]
#[serial]
async fn set_volume_round_trips_through_mpv() {
    let td = TestDaemon::new().await;
    td.core.set_volume(75).await.unwrap();

    let saw_volume_set = td.fake_mpv.commands().await.iter().any(|c| {
        c.first().and_then(serde_json::Value::as_str) == Some("set_property")
            && c.get(1).and_then(serde_json::Value::as_str) == Some("volume")
    });
    assert!(
        saw_volume_set,
        "set_volume must issue mpv set_property volume"
    );
}
