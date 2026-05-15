//! audio/mpv.rs: every public method dispatched through fake mpv.

mod common;

use common::fake_mpv::FakeMpv;
use ferrosonic::audio::mpv::MpvController;
use serial_test::serial;

async fn ctrl_and_fake() -> (MpvController, FakeMpv) {
    let fake = FakeMpv::start().await;
    let mut ctrl = MpvController::with_socket_path(fake.socket_path.clone());
    ctrl.connect_to_existing().await.unwrap();
    (ctrl, fake)
}

#[tokio::test]
#[serial]
async fn loadfile_replace_succeeds() {
    let (mut ctrl, fake) = ctrl_and_fake().await;
    ctrl.loadfile("/tmp/test.mp3").await.unwrap();
    assert_eq!(fake.loaded_file().await.as_deref(), Some("/tmp/test.mp3"));
    assert_eq!(fake.playlist().await, vec!["/tmp/test.mp3".to_string()]);
}

#[tokio::test]
#[serial]
async fn loadfile_append_succeeds() {
    let (mut ctrl, fake) = ctrl_and_fake().await;
    ctrl.loadfile_append("/tmp/test.mp3").await.unwrap();
    assert_eq!(fake.playlist().await, vec!["/tmp/test.mp3".to_string()]);
    assert!(fake
        .commands()
        .await
        .iter()
        .any(|c| c.first().and_then(|v| v.as_str()) == Some("loadfile")
            && c.get(2).and_then(|v| v.as_str()) == Some("append")));
}

#[tokio::test]
#[serial]
async fn playlist_remove_succeeds() {
    let (mut ctrl, fake) = ctrl_and_fake().await;
    fake.set_playlist(vec!["a".into(), "b".into()]).await;
    ctrl.playlist_remove(0).await.unwrap();
    assert_eq!(fake.playlist().await, vec!["b".to_string()]);
}

#[tokio::test]
#[serial]
async fn playlist_next_succeeds() {
    let (mut ctrl, fake) = ctrl_and_fake().await;
    fake.set_playlist(vec!["a".into(), "b".into()]).await;
    ctrl.playlist_next().await.unwrap();
    assert_eq!(fake.playlist().await, vec!["b".to_string()]);
    assert_eq!(fake.loaded_file().await.as_deref(), Some("b"));
}

#[tokio::test]
#[serial]
async fn get_playlist_pos_returns_option() {
    let (mut ctrl, fake) = ctrl_and_fake().await;
    fake.set_playlist_pos(3).await;
    let pos = ctrl.get_playlist_pos().await.unwrap();
    assert_eq!(pos, Some(3));
}

#[tokio::test]
#[serial]
async fn get_playlist_count_returns_count() {
    let (mut ctrl, fake) = ctrl_and_fake().await;
    fake.set_playlist(vec!["a".into(), "b".into(), "c".into()])
        .await;
    let count = ctrl.get_playlist_count().await.unwrap();
    assert_eq!(count, 3);
}

#[tokio::test]
#[serial]
async fn pause_succeeds() {
    let (mut ctrl, fake) = ctrl_and_fake().await;
    ctrl.pause().await.unwrap();
    assert!(fake.is_paused().await);
}

#[tokio::test]
#[serial]
async fn resume_succeeds() {
    let (mut ctrl, fake) = ctrl_and_fake().await;
    ctrl.pause().await.unwrap();
    ctrl.resume().await.unwrap();
    assert!(!fake.is_paused().await);
}

#[tokio::test]
#[serial]
async fn toggle_pause_returns_new_state() {
    let (mut ctrl, fake) = ctrl_and_fake().await;
    let new_state = ctrl.toggle_pause().await.unwrap();
    assert!(new_state);
    assert!(fake.is_paused().await);
}

#[tokio::test]
#[serial]
async fn is_paused_returns_bool() {
    let (mut ctrl, fake) = ctrl_and_fake().await;
    let before = ctrl.is_paused().await.unwrap();
    assert!(!before);
    ctrl.pause().await.unwrap();
    let after = ctrl.is_paused().await.unwrap();
    assert!(after);
    assert!(fake.is_paused().await);
}

#[tokio::test]
#[serial]
async fn stop_succeeds() {
    let (mut ctrl, fake) = ctrl_and_fake().await;
    fake.set_loaded_file("/tmp/x.mp3").await;
    ctrl.stop().await.unwrap();
    assert_eq!(fake.loaded_file().await, None);
    assert!(fake.playlist().await.is_empty());
}

#[tokio::test]
#[serial]
async fn seek_absolute_succeeds() {
    let (mut ctrl, fake) = ctrl_and_fake().await;
    ctrl.seek(30.5).await.unwrap();
    assert!((fake.position().await - 30.5).abs() < f64::EPSILON);
}

#[tokio::test]
#[serial]
async fn seek_relative_positive_succeeds() {
    let (mut ctrl, fake) = ctrl_and_fake().await;
    fake.set_position(20.0).await;
    ctrl.seek_relative(10.0).await.unwrap();
    assert!((fake.position().await - 30.0).abs() < f64::EPSILON);
}

#[tokio::test]
#[serial]
async fn seek_relative_negative_succeeds() {
    let (mut ctrl, fake) = ctrl_and_fake().await;
    fake.set_position(20.0).await;
    ctrl.seek_relative(-5.0).await.unwrap();
    assert!((fake.position().await - 15.0).abs() < f64::EPSILON);
}

#[tokio::test]
#[serial]
async fn get_time_pos_returns_float() {
    let (mut ctrl, fake) = ctrl_and_fake().await;
    fake.set_position(42.0).await;
    let pos = ctrl.get_time_pos().await.unwrap();
    assert!((pos - 42.0).abs() < f64::EPSILON);
}

#[tokio::test]
#[serial]
async fn get_duration_returns_float() {
    let (mut ctrl, fake) = ctrl_and_fake().await;
    fake.set_duration(123.0).await;
    let dur = ctrl.get_duration().await.unwrap();
    assert!((dur - 123.0).abs() < f64::EPSILON);
}

#[tokio::test]
#[serial]
async fn set_volume_clamps_negative_to_zero() {
    let (mut ctrl, fake) = ctrl_and_fake().await;
    ctrl.set_volume(-50).await.unwrap();
    let saw_zero = fake.commands().await.iter().any(|c| {
        c.first().and_then(|v| v.as_str()) == Some("set_property")
            && c.get(1).and_then(|v| v.as_str()) == Some("volume")
            && c.get(2).and_then(|v| v.as_i64()) == Some(0)
    });
    assert!(saw_zero, "expected set_property volume 0 in commands");
}

#[tokio::test]
#[serial]
async fn set_volume_clamps_above_hundred() {
    let (mut ctrl, fake) = ctrl_and_fake().await;
    ctrl.set_volume(250).await.unwrap();
    let saw_max = fake.commands().await.iter().any(|c| {
        c.first().and_then(|v| v.as_str()) == Some("set_property")
            && c.get(1).and_then(|v| v.as_str()) == Some("volume")
            && c.get(2).and_then(|v| v.as_i64()) == Some(100)
    });
    assert!(saw_max, "expected set_property volume 100 in commands");
}

#[tokio::test]
#[serial]
async fn set_volume_within_range_succeeds() {
    let (mut ctrl, fake) = ctrl_and_fake().await;
    ctrl.set_volume(50).await.unwrap();
    let saw_fifty = fake.commands().await.iter().any(|c| {
        c.first().and_then(|v| v.as_str()) == Some("set_property")
            && c.get(1).and_then(|v| v.as_str()) == Some("volume")
            && c.get(2).and_then(|v| v.as_i64()) == Some(50)
    });
    assert!(saw_fifty, "expected set_property volume 50 in commands");
}

#[tokio::test]
#[serial]
async fn get_sample_rate_returns_option() {
    let (mut ctrl, fake) = ctrl_and_fake().await;
    fake.set_property("audio-params/samplerate", serde_json::json!(48000))
        .await;
    let rate = ctrl.get_sample_rate().await.unwrap();
    assert_eq!(rate, Some(48000));
}

#[tokio::test]
#[serial]
async fn get_bit_depth_returns_option() {
    let (mut ctrl, fake) = ctrl_and_fake().await;
    fake.set_property("audio-params/format", serde_json::json!("s24"))
        .await;
    let depth = ctrl.get_bit_depth().await.unwrap();
    assert_eq!(depth, Some(24));
}

#[tokio::test]
#[serial]
async fn get_audio_format_returns_option() {
    let (mut ctrl, fake) = ctrl_and_fake().await;
    fake.set_property("audio-params/format", serde_json::json!("s16"))
        .await;
    let fmt = ctrl.get_audio_format().await.unwrap();
    assert_eq!(fmt.as_deref(), Some("s16"));
}

#[tokio::test]
#[serial]
async fn get_channels_returns_option() {
    let (mut ctrl, fake) = ctrl_and_fake().await;
    fake.set_property("audio-params/channel-count", serde_json::json!(2))
        .await;
    let chans = ctrl.get_channels().await.unwrap();
    assert_eq!(chans.as_deref(), Some("Stereo"));
}

#[tokio::test]
#[serial]
async fn is_idle_returns_bool() {
    let (mut ctrl, fake) = ctrl_and_fake().await;
    let idle = ctrl.is_idle().await.unwrap();
    assert!(idle, "no loaded file means idle-active true");
    fake.set_loaded_file("/tmp/x.mp3").await;
    let not_idle = ctrl.is_idle().await.unwrap();
    assert!(!not_idle, "loaded file means idle-active false");
}

#[tokio::test]
#[serial]
async fn quit_via_writer_present_path() {
    let (mut ctrl, fake) = ctrl_and_fake().await;
    ctrl.quit().await.unwrap();
    let saw_quit = fake
        .commands()
        .await
        .iter()
        .any(|c| c.first().and_then(|v| v.as_str()) == Some("quit"));
    assert!(saw_quit, "expected quit command recorded");
}

#[tokio::test]
#[serial]
async fn quit_with_no_writer_returns_ok() {
    let mut ctrl = MpvController::new();
    let result = ctrl.quit().await;
    assert!(result.is_ok(), "quit with no writer must return Ok");
    assert!(!ctrl.is_running(), "controller must be shut down after quit");
}

#[tokio::test]
#[serial]
async fn is_running_with_no_process_and_no_ipc_returns_false() {
    let mut ctrl = MpvController::new();
    assert!(!ctrl.is_running());
}
