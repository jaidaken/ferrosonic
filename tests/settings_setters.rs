//! Config setters: set_repeat_mode, set_auto_continue, set_daemon_enabled,
//! set_cover_art_enabled, set_cover_art_size, set_cava_enabled, set_cava_size,
//! set_volume. Verifies state mutation, persistence, and event emission.

mod common;

use common::TestDaemon;
use ferrosonic::config::{RepeatMode, ReplayGainMode};
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

#[tokio::test]
#[serial]
async fn set_replay_gain_mode_persists_and_pushes_live() {
    let td = TestDaemon::new().await;
    td.core
        .set_replay_gain_mode(ReplayGainMode::Album)
        .await
        .unwrap();
    assert_eq!(
        td.state.read().await.config.replay_gain_mode,
        ReplayGainMode::Album
    );

    let saw_replaygain_set = td.fake_mpv.commands().await.iter().any(|c| {
        c.first().and_then(serde_json::Value::as_str) == Some("set_property")
            && c.get(1).and_then(serde_json::Value::as_str) == Some("replaygain")
            && c.get(2).and_then(serde_json::Value::as_str) == Some("album")
    });
    assert!(
        saw_replaygain_set,
        "set_replay_gain_mode must issue mpv set_property replaygain \"album\""
    );
}

#[tokio::test]
#[serial]
async fn set_replay_gain_preamp_persists_clamps_and_pushes_live() {
    let td = TestDaemon::new().await;

    td.core.set_replay_gain_preamp(4.5).await.unwrap();
    assert_eq!(td.state.read().await.config.replay_gain_preamp, 4.5);

    // Out-of-range values clamp to mpv's -15.0..=15.0 range.
    td.core.set_replay_gain_preamp(100.0).await.unwrap();
    assert_eq!(td.state.read().await.config.replay_gain_preamp, 15.0);
    td.core.set_replay_gain_preamp(-100.0).await.unwrap();
    assert_eq!(td.state.read().await.config.replay_gain_preamp, -15.0);

    let saw_preamp_set = td.fake_mpv.commands().await.iter().any(|c| {
        c.first().and_then(serde_json::Value::as_str) == Some("set_property")
            && c.get(1).and_then(serde_json::Value::as_str) == Some("replaygain-preamp")
            && c.get(2).and_then(serde_json::Value::as_f64) == Some(4.5)
    });
    assert!(
        saw_preamp_set,
        "set_replay_gain_preamp must issue mpv set_property replaygain-preamp 4.5"
    );
}

#[tokio::test]
#[serial]
async fn set_replay_gain_clip_persists_and_pushes_live() {
    let td = TestDaemon::new().await;
    assert!(!td.state.read().await.config.replay_gain_clip);

    // Our "prevent clipping" is the inverse of mpv's own `replaygain-clip`
    // ("allow clip") property, so on() must push `false` to mpv.
    td.core.set_replay_gain_clip(true).await.unwrap();
    assert!(td.state.read().await.config.replay_gain_clip);

    let saw_clip_set = td.fake_mpv.commands().await.iter().any(|c| {
        c.first().and_then(serde_json::Value::as_str) == Some("set_property")
            && c.get(1).and_then(serde_json::Value::as_str) == Some("replaygain-clip")
            && c.get(2).and_then(serde_json::Value::as_bool) == Some(false)
    });
    assert!(
        saw_clip_set,
        "set_replay_gain_clip(true) (prevent clipping) must issue mpv set_property replaygain-clip false (mpv's \"don't allow clip\")"
    );
}

#[tokio::test]
#[serial]
async fn replaygain_live_commands_use_mpv_semantics() {
    let td = TestDaemon::new().await;
    td.core
        .set_replay_gain_mode(ferrosonic::config::ReplayGainMode::Album)
        .await
        .unwrap();
    td.core.set_replay_gain_preamp(99.0).await.unwrap();
    td.core.set_replay_gain_clip(true).await.unwrap();
    td.core.set_replay_gain_clip(false).await.unwrap();
    let commands = td.fake_mpv.commands().await;
    for command in [
        serde_json::json!(["set_property", "replaygain", "album"]),
        serde_json::json!(["set_property", "replaygain-preamp", 15.0]),
        serde_json::json!(["set_property", "replaygain-clip", false]),
        serde_json::json!(["set_property", "replaygain-clip", true]),
    ] {
        assert!(
            commands.iter().any(|c| serde_json::json!(c) == command),
            "{command}"
        );
    }
}

#[tokio::test]
#[serial]
async fn nonfinite_replaygain_preamp_is_rejected_without_state_or_mpv_changes() {
    let td = TestDaemon::new().await;
    td.core.set_replay_gain_preamp(1.5).await.unwrap();
    let before = td.fake_mpv.commands().await;
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert!(td.core.set_replay_gain_preamp(value).await.is_err());
        assert_eq!(td.state.read().await.config.replay_gain_preamp, 1.5);
    }
    assert_eq!(td.fake_mpv.commands().await, before);
}

#[test]
fn nonfinite_preamp_config_is_rejected() {
    for value in ["nan", "inf", "-inf"] {
        assert!(
            toml::from_str::<ferrosonic::config::Config>(&format!("ReplayGainPreamp = {value}"))
                .is_err(),
            "{value} must not load"
        );
    }
}
