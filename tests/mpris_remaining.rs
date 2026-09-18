//! MPRIS PlayerInterface + RootInterface getters/setters not yet covered.

mod common;

use std::sync::Arc;

use common::RecordingClient;
use ferrosonic::app::state::{new_shared_client_state, new_shared_daemon_state, SharedDaemonState};
use ferrosonic::config::{Config, RepeatMode};
use ferrosonic::ipc::protocol::DaemonRequest;
use ferrosonic::mpris::server::MprisPlayer;
use mpris_server::{LoopStatus, PlaybackRate, PlayerInterface, RootInterface};
use serial_test::serial;

fn build_player() -> (MprisPlayer, Arc<RecordingClient>, SharedDaemonState) {
    let config = Config::new();
    let daemon_state = new_shared_daemon_state(config.clone());
    let client_state = new_shared_client_state(&config);
    let rec = RecordingClient::new();
    let player = MprisPlayer::new(daemon_state.clone(), client_state, rec.clone());
    (player, rec, daemon_state)
}

#[tokio::test]
async fn loop_status_returns_none() {
    let (player, _, _) = build_player();
    assert_eq!(player.loop_status().await.unwrap(), LoopStatus::None);
}

#[tokio::test]
async fn set_loop_status_dispatches_repeat_mode_change() {
    let (player, rec, _) = build_player();
    player.set_loop_status(LoopStatus::Track).await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    assert!(
        rec.requests()
            .await
            .iter()
            .any(|r| matches!(r, DaemonRequest::SetRepeatMode(RepeatMode::One))),
        "MPRIS Track loop must map to repeat-one"
    );
}

#[tokio::test]
async fn shuffle_status_returns_false() {
    let (player, _, _) = build_player();
    assert!(!player.shuffle().await.unwrap());
}

#[tokio::test]
async fn set_shuffle_is_silent_noop() {
    let (player, _, _) = build_player();
    player.set_shuffle(true).await.unwrap();
    assert!(!player.shuffle().await.unwrap());
}

#[tokio::test]
async fn rate_returns_one() {
    let (player, _, _) = build_player();
    let rate: PlaybackRate = player.rate().await.unwrap();
    assert!((rate - 1.0).abs() < 1e-9);
}

#[tokio::test]
async fn minimum_and_maximum_rate_match() {
    let (player, _, _) = build_player();
    assert!((player.minimum_rate().await.unwrap() - 1.0).abs() < 1e-9);
    assert!((player.maximum_rate().await.unwrap() - 1.0).abs() < 1e-9);
}

#[tokio::test]
async fn volume_returns_one() {
    let (player, _, _) = build_player();
    let v = player.volume().await.unwrap();
    assert!((v - 1.0).abs() < 1e-9);
}

#[tokio::test]
async fn set_volume_round_trips_through_the_getter() {
    let (player, rec, _) = build_player();
    player.set_volume(0.4).await.unwrap();
    let v = player.volume().await.unwrap();
    assert!(
        (v - 0.4).abs() < 1e-9,
        "getter must reflect SetVolume, got {v}"
    );
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    assert!(
        rec.requests()
            .await
            .iter()
            .any(|r| matches!(r, DaemonRequest::SetVolume(40))),
        "0.4 must dispatch 40 percent to the daemon"
    );
}

#[tokio::test]
async fn set_volume_clamps_out_of_range_values() {
    let (player, _, _) = build_player();
    player.set_volume(3.0).await.unwrap();
    assert!((player.volume().await.unwrap() - 1.0).abs() < 1e-9);
    player.set_volume(-1.0).await.unwrap();
    assert!(player.volume().await.unwrap().abs() < 1e-9);
    player.set_volume(f64::NAN).await.unwrap();
    assert!(player.volume().await.unwrap().abs() < 1e-9);
    player.set_volume(f64::INFINITY).await.unwrap();
    assert!((player.volume().await.unwrap() - 1.0).abs() < 1e-9);
    player.set_volume(f64::NEG_INFINITY).await.unwrap();
    assert!(player.volume().await.unwrap().abs() < 1e-9);
}

#[tokio::test]
#[serial]
async fn volume_getter_reflects_non_mpris_daemon_updates() {
    let td = common::TestDaemon::new().await;
    let config = td.state.read().await.config.clone();
    let player = MprisPlayer::new(
        td.state.clone(),
        new_shared_client_state(&config),
        Arc::new(ferrosonic::ipc::InProcessClient::new(td.core.clone())),
    );
    td.core.set_volume(37).await.unwrap();
    assert!((player.volume().await.unwrap() - 0.37).abs() < 1e-9);
}

#[tokio::test]
async fn metadata_getter_never_exposes_the_authenticated_remote_url() {
    let (player, _, daemon_state) = build_player();
    {
        let mut ds = daemon_state.write().await;
        ds.config.base_url = "https://nav.example".into();
        ds.config.username = "u".into();
        ds.config.password = "p".into();
        let mut sng = common::song("track-1", "Track");
        sng.cover_art = Some("cover-1".into());
        ds.queue = vec![sng.clone()];
        ds.queue_position = Some(0);
        ds.now_playing.song = Some(sng);
    }
    let md = player.metadata().await.unwrap();
    assert!(
        md.art_url().is_none(),
        "the MPRIS Metadata getter must not publish a token-bearing remote art URL"
    );
}

#[tokio::test]
async fn can_pause_always_true() {
    let (player, _, _) = build_player();
    assert!(player.can_pause().await.unwrap());
}

#[tokio::test]
async fn can_seek_always_true() {
    let (player, _, _) = build_player();
    assert!(player.can_seek().await.unwrap());
}

#[tokio::test]
async fn can_control_always_true() {
    let (player, _, _) = build_player();
    assert!(player.can_control().await.unwrap());
}

#[tokio::test]
async fn open_uri_returns_ok_silently() {
    let (player, _, _) = build_player();
    let result = player.open_uri("http://nope.example".into()).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn root_identity_is_ferrosonic() {
    let (player, _, _) = build_player();
    let id = player.identity().await.unwrap();
    assert!(id.to_lowercase().contains("ferrosonic"));
}

#[tokio::test]
async fn root_desktop_entry_is_ferrosonic() {
    let (player, _, _) = build_player();
    assert_eq!(player.desktop_entry().await.unwrap(), "ferrosonic");
}

#[tokio::test]
async fn root_can_quit_returns_true() {
    let (player, _, _) = build_player();
    assert!(player.can_quit().await.unwrap());
}

#[tokio::test]
async fn root_quit_sets_should_quit_flag() {
    let config = Config::new();
    let daemon_state = new_shared_daemon_state(config.clone());
    let client_state = new_shared_client_state(&config);
    let rec = RecordingClient::new();
    let player = MprisPlayer::new(daemon_state, client_state.clone(), rec);
    player.quit().await.unwrap();
    assert!(client_state.read().await.should_quit);
}

#[tokio::test]
async fn root_raise_is_silent_noop() {
    let (player, _, _) = build_player();
    let result = player.raise().await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn root_fullscreen_is_false() {
    let (player, _, _) = build_player();
    assert!(!player.fullscreen().await.unwrap());
}

#[tokio::test]
async fn root_can_set_fullscreen_is_false() {
    let (player, _, _) = build_player();
    assert!(!player.can_set_fullscreen().await.unwrap());
}

#[tokio::test]
async fn root_set_fullscreen_is_silent_noop() {
    let (player, _, _) = build_player();
    let result = player.set_fullscreen(true).await;
    assert!(result.is_ok());
    assert!(!player.fullscreen().await.unwrap());
}

#[tokio::test]
async fn root_can_raise_is_false() {
    let (player, _, _) = build_player();
    assert!(!player.can_raise().await.unwrap());
}

#[tokio::test]
async fn root_has_track_list_is_false() {
    let (player, _, _) = build_player();
    assert!(!player.has_track_list().await.unwrap());
}

#[tokio::test]
async fn root_supported_uri_schemes_lists_http() {
    let (player, _, _) = build_player();
    let schemes = player.supported_uri_schemes().await.unwrap();
    assert!(schemes.iter().any(|s| s == "http" || s == "https"));
}

#[tokio::test]
async fn root_supported_mime_types_lists_audio() {
    let (player, _, _) = build_player();
    let mimes = player.supported_mime_types().await.unwrap();
    assert!(mimes.iter().any(|m| m.starts_with("audio/")));
}

#[tokio::test]
async fn rating_getter_and_pushed_metadata_agree() {
    let (player, _, ds) = build_player();
    for rating in [Some(1), Some(5), None] {
        let mut song = common::song("rated", "Rated");
        song.user_rating = rating;
        song.track = Some(3);
        song.disc_number = Some(2);
        {
            let mut state = ds.write().await;
            state.queue = vec![song.clone()];
            state.queue_position = Some(0);
            state.now_playing.song = Some(song);
        }
        let getter = player.metadata().await.unwrap();
        let pushed = ferrosonic::mpris::server::build_property_snapshot(&ds)
            .await
            .metadata
            .unwrap();
        assert_eq!(getter.user_rating(), rating.map(|r| f64::from(r) / 5.0));
        assert_eq!(getter.user_rating(), pushed.user_rating());
        assert_eq!(getter.track_number(), pushed.track_number());
        assert_eq!(getter.disc_number(), pushed.disc_number());
    }
}
