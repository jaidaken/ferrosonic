//! MPRIS dispatch: PlayerInterface methods route to the right DaemonRequest.

mod common;

use std::sync::Arc;

use common::RecordingClient;
use ferrosonic::app::state::{
    new_shared_client_state, new_shared_daemon_state, SharedDaemonState,
};
use ferrosonic::daemon::state::PlaybackState;
use ferrosonic::config::Config;
use ferrosonic::ipc::protocol::DaemonRequest;
use ferrosonic::mpris::server::MprisPlayer;
use mpris_server::{PlaybackStatus, PlayerInterface, Time, TrackId};

fn build_player() -> (MprisPlayer, Arc<RecordingClient>, SharedDaemonState) {
    let config = Config::new();
    let daemon_state = new_shared_daemon_state(config.clone());
    let client_state = new_shared_client_state(&config);
    let rec = RecordingClient::new();
    let player = MprisPlayer::new(daemon_state.clone(), client_state, rec.clone());
    (player, rec, daemon_state)
}

async fn drain_fire(rec: &RecordingClient, expected: usize) {
    for _ in 0..500 {
        if rec.requests().await.len() >= expected {
            return;
        }
        tokio::task::yield_now().await;
    }
}

#[tokio::test]
async fn mpris_stop_dispatches_daemon_stop() {
    let (player, rec, _) = build_player();
    player.stop().await.unwrap();
    drain_fire(&rec, 1).await;
    let reqs = rec.requests().await;
    assert!(
        matches!(reqs.as_slice(), [DaemonRequest::Stop]),
        "MPRIS Stop must dispatch DaemonRequest::Stop; got {:?}",
        reqs
    );
}

#[tokio::test]
async fn mpris_play_dispatches_resume() {
    let (player, rec, _) = build_player();
    player.play().await.unwrap();
    drain_fire(&rec, 1).await;
    assert!(matches!(
        rec.requests().await.as_slice(),
        [DaemonRequest::Resume]
    ));
}

#[tokio::test]
async fn mpris_pause_dispatches_pause() {
    let (player, rec, _) = build_player();
    player.pause().await.unwrap();
    drain_fire(&rec, 1).await;
    assert!(matches!(
        rec.requests().await.as_slice(),
        [DaemonRequest::Pause]
    ));
}

#[tokio::test]
async fn mpris_play_pause_dispatches_toggle() {
    let (player, rec, _) = build_player();
    player.play_pause().await.unwrap();
    drain_fire(&rec, 1).await;
    assert!(matches!(
        rec.requests().await.as_slice(),
        [DaemonRequest::TogglePause]
    ));
}

#[tokio::test]
async fn mpris_next_and_previous_dispatch_correctly() {
    let (player, rec, _) = build_player();
    player.next().await.unwrap();
    player.previous().await.unwrap();
    drain_fire(&rec, 2).await;
    let reqs = rec.requests().await;
    assert!(matches!(
        reqs.as_slice(),
        [DaemonRequest::Next, DaemonRequest::Previous]
    ));
}

#[tokio::test]
async fn mpris_seek_dispatches_seek_relative_with_seconds() {
    let (player, rec, _) = build_player();
    player.seek(Time::from_micros(2_500_000)).await.unwrap();
    drain_fire(&rec, 1).await;
    let reqs = rec.requests().await;
    match reqs.as_slice() {
        [DaemonRequest::SeekRelative(s)] => {
            assert!((*s - 2.5).abs() < 1e-6, "expected 2.5s, got {}", s);
        }
        other => panic!("expected SeekRelative(2.5), got {:?}", other),
    }
}

#[tokio::test]
async fn mpris_set_position_dispatches_absolute_seek_with_seconds() {
    let (player, rec, _) = build_player();
    let track = TrackId::try_from("/org/mpris/MediaPlayer2/Track/x").unwrap();
    player
        .set_position(track, Time::from_micros(2_500_000))
        .await
        .unwrap();
    drain_fire(&rec, 1).await;
    match rec.requests().await.as_slice() {
        [DaemonRequest::Seek(s)] => assert!((*s - 2.5).abs() < 1e-6, "expected 2.5s, got {}", s),
        other => panic!("expected Seek(2.5), got {:?}", other),
    }
}

#[tokio::test]
async fn mpris_metadata_reflects_now_playing() {
    let (player, _, daemon_state) = build_player();
    {
        let mut s = daemon_state.write().await;
        let mut sng = common::song("abc", "Lullaby");
        sng.artist = Some("The Cure".into());
        sng.album = Some("Disintegration".into());
        sng.duration = Some(243);
        s.queue.push(sng.clone());
        s.queue_position = Some(0);
        s.now_playing.song = Some(sng);
    }

    let md = player.metadata().await.unwrap();
    let title = md.title().map(String::from);
    let artist = md.artist().map(|a| a.first().cloned());
    let album = md.album().map(String::from);
    assert_eq!(title.as_deref(), Some("Lullaby"));
    assert_eq!(artist.unwrap_or_default().as_deref(), Some("The Cure"));
    assert_eq!(album.as_deref(), Some("Disintegration"));
}

#[tokio::test]
async fn mpris_can_go_next_and_previous_track_queue_position() {
    let (player, _, daemon_state) = build_player();
    {
        let mut s = daemon_state.write().await;
        s.queue = common::songs("t", 3);
        s.queue_position = Some(1);
    }

    assert!(player.can_go_next().await.unwrap());
    assert!(player.can_go_previous().await.unwrap());

    {
        let mut s = daemon_state.write().await;
        s.queue_position = Some(0);
    }
    assert!(player.can_go_next().await.unwrap());
    assert!(!player.can_go_previous().await.unwrap());

    {
        let mut s = daemon_state.write().await;
        s.queue_position = Some(2);
    }
    assert!(!player.can_go_next().await.unwrap());
    assert!(player.can_go_previous().await.unwrap());
}

#[tokio::test]
async fn mpris_can_play_tracks_queue_non_empty() {
    let (player, _, daemon_state) = build_player();
    assert!(
        !player.can_play().await.unwrap(),
        "empty queue: cannot play"
    );

    {
        let mut s = daemon_state.write().await;
        s.queue.push(common::song("a", "A"));
    }
    assert!(player.can_play().await.unwrap());
}

#[tokio::test]
async fn mpris_set_volume_dispatches_set_volume_request() {
    let (player, rec, _) = build_player();
    player.set_volume(0.7).await.unwrap();
    drain_fire(&rec, 1).await;
    match rec.requests().await.as_slice() {
        [DaemonRequest::SetVolume(v)] => assert_eq!(*v, 70),
        other => panic!("expected SetVolume(70), got {:?}", other),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn mpris_handler_off_tokio_runtime_does_not_panic() {
    // Regression for #27: zbus drives handlers off the tokio runtime, where
    // the old fire()'s tokio::spawn panicked ("there is no reactor running").
    let (player, rec, _) = build_player();
    let player = Arc::new(player);
    let p = player.clone();
    let result = std::thread::spawn(move || futures::executor::block_on(p.next()))
        .join()
        .expect("MPRIS handler must not panic when invoked off the tokio runtime");
    assert!(result.is_ok(), "off-runtime next() errored: {:?}", result);
    drain_fire(&rec, 1).await;
    assert!(
        matches!(rec.requests().await.as_slice(), [DaemonRequest::Next]),
        "off-runtime next() must still dispatch DaemonRequest::Next"
    );
}

#[tokio::test]
async fn mpris_position_reflects_daemon_state_in_microseconds() {
    let (player, _, daemon_state) = build_player();
    {
        let mut s = daemon_state.write().await;
        s.now_playing.position = 12.5;
    }
    let pos = player.position().await.unwrap();
    assert_eq!(pos.as_micros(), 12_500_000);
}

#[tokio::test]
async fn mpris_playback_status_reports_daemon_state() {
    let (player, _rec, daemon_state) = build_player();

    {
        let mut s = daemon_state.write().await;
        s.now_playing.state = PlaybackState::Playing;
    }
    assert_eq!(
        player.playback_status().await.unwrap(),
        PlaybackStatus::Playing
    );

    {
        let mut s = daemon_state.write().await;
        s.now_playing.state = PlaybackState::Paused;
    }
    assert_eq!(
        player.playback_status().await.unwrap(),
        PlaybackStatus::Paused
    );

    {
        let mut s = daemon_state.write().await;
        s.now_playing.state = PlaybackState::Stopped;
    }
    assert_eq!(
        player.playback_status().await.unwrap(),
        PlaybackStatus::Stopped
    );
}
