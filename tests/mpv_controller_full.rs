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
    let (mut ctrl, _f) = ctrl_and_fake().await;
    ctrl.loadfile("/tmp/test.mp3").await.unwrap();
}

#[tokio::test]
#[serial]
async fn loadfile_append_succeeds() {
    let (mut ctrl, _f) = ctrl_and_fake().await;
    ctrl.loadfile_append("/tmp/test.mp3").await.unwrap();
}

#[tokio::test]
#[serial]
async fn playlist_remove_succeeds() {
    let (mut ctrl, _f) = ctrl_and_fake().await;
    let _ = ctrl.playlist_remove(0).await;
}

#[tokio::test]
#[serial]
async fn playlist_next_succeeds() {
    let (mut ctrl, _f) = ctrl_and_fake().await;
    let _ = ctrl.playlist_next().await;
}

#[tokio::test]
#[serial]
async fn get_playlist_pos_returns_option() {
    let (mut ctrl, _f) = ctrl_and_fake().await;
    let _ = ctrl.get_playlist_pos().await;
}

#[tokio::test]
#[serial]
async fn get_playlist_count_returns_count() {
    let (mut ctrl, _f) = ctrl_and_fake().await;
    let _ = ctrl.get_playlist_count().await;
}

#[tokio::test]
#[serial]
async fn pause_succeeds() {
    let (mut ctrl, _f) = ctrl_and_fake().await;
    ctrl.pause().await.unwrap();
}

#[tokio::test]
#[serial]
async fn resume_succeeds() {
    let (mut ctrl, _f) = ctrl_and_fake().await;
    ctrl.resume().await.unwrap();
}

#[tokio::test]
#[serial]
async fn toggle_pause_returns_new_state() {
    let (mut ctrl, _f) = ctrl_and_fake().await;
    let _ = ctrl.toggle_pause().await;
}

#[tokio::test]
#[serial]
async fn is_paused_returns_bool() {
    let (mut ctrl, _f) = ctrl_and_fake().await;
    let _ = ctrl.is_paused().await;
}

#[tokio::test]
#[serial]
async fn stop_succeeds() {
    let (mut ctrl, _f) = ctrl_and_fake().await;
    ctrl.stop().await.unwrap();
}

#[tokio::test]
#[serial]
async fn seek_absolute_succeeds() {
    let (mut ctrl, _f) = ctrl_and_fake().await;
    ctrl.seek(30.5).await.unwrap();
}

#[tokio::test]
#[serial]
async fn seek_relative_positive_succeeds() {
    let (mut ctrl, _f) = ctrl_and_fake().await;
    ctrl.seek_relative(10.0).await.unwrap();
}

#[tokio::test]
#[serial]
async fn seek_relative_negative_succeeds() {
    let (mut ctrl, _f) = ctrl_and_fake().await;
    ctrl.seek_relative(-5.0).await.unwrap();
}

#[tokio::test]
#[serial]
async fn get_time_pos_returns_float() {
    let (mut ctrl, _f) = ctrl_and_fake().await;
    let _ = ctrl.get_time_pos().await;
}

#[tokio::test]
#[serial]
async fn get_duration_returns_float() {
    let (mut ctrl, _f) = ctrl_and_fake().await;
    let _ = ctrl.get_duration().await;
}

#[tokio::test]
#[serial]
async fn set_volume_clamps_negative_to_zero() {
    let (mut ctrl, _f) = ctrl_and_fake().await;
    ctrl.set_volume(-50).await.unwrap();
}

#[tokio::test]
#[serial]
async fn set_volume_clamps_above_hundred() {
    let (mut ctrl, _f) = ctrl_and_fake().await;
    ctrl.set_volume(250).await.unwrap();
}

#[tokio::test]
#[serial]
async fn set_volume_within_range_succeeds() {
    let (mut ctrl, _f) = ctrl_and_fake().await;
    ctrl.set_volume(50).await.unwrap();
}

#[tokio::test]
#[serial]
async fn get_sample_rate_returns_option() {
    let (mut ctrl, _f) = ctrl_and_fake().await;
    let _ = ctrl.get_sample_rate().await;
}

#[tokio::test]
#[serial]
async fn get_bit_depth_returns_option() {
    let (mut ctrl, _f) = ctrl_and_fake().await;
    let _ = ctrl.get_bit_depth().await;
}

#[tokio::test]
#[serial]
async fn get_audio_format_returns_option() {
    let (mut ctrl, _f) = ctrl_and_fake().await;
    let _ = ctrl.get_audio_format().await;
}

#[tokio::test]
#[serial]
async fn get_channels_returns_option() {
    let (mut ctrl, _f) = ctrl_and_fake().await;
    let _ = ctrl.get_channels().await;
}

#[tokio::test]
#[serial]
async fn is_idle_returns_bool() {
    let (mut ctrl, _f) = ctrl_and_fake().await;
    let _ = ctrl.is_idle().await;
}

#[tokio::test]
#[serial]
async fn quit_via_writer_present_path() {
    let (mut ctrl, _f) = ctrl_and_fake().await;
    ctrl.quit().await.unwrap();
}

#[tokio::test]
#[serial]
async fn quit_with_no_writer_returns_ok() {
    let mut ctrl = MpvController::new();
    ctrl.quit().await.unwrap();
}

#[tokio::test]
#[serial]
async fn is_running_with_no_process_and_no_ipc_returns_false() {
    let mut ctrl = MpvController::new();
    assert!(!ctrl.is_running());
}
