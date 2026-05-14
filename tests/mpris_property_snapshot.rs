//! MPRIS update_mpris_properties via the pure build_property_snapshot.

mod common;

use common::song;
use ferrosonic::app::state::new_shared_daemon_state;
use ferrosonic::daemon::state::PlaybackState;
use ferrosonic::config::Config;
use ferrosonic::mpris::server::build_property_snapshot;
use mpris_server::PlaybackStatus;
use serial_test::serial;

#[tokio::test]
#[serial]
async fn playing_state_maps_to_playing_status() {
    let ds = new_shared_daemon_state(Config::new());
    {
        let mut s = ds.write().await;
        s.now_playing.state = PlaybackState::Playing;
    }
    let snap = build_property_snapshot(&ds).await;
    assert_eq!(snap.playback, PlaybackStatus::Playing);
}

#[tokio::test]
#[serial]
async fn paused_state_maps_to_paused_status() {
    let ds = new_shared_daemon_state(Config::new());
    {
        let mut s = ds.write().await;
        s.now_playing.state = PlaybackState::Paused;
    }
    let snap = build_property_snapshot(&ds).await;
    assert_eq!(snap.playback, PlaybackStatus::Paused);
}

#[tokio::test]
#[serial]
async fn stopped_state_maps_to_stopped_status() {
    let ds = new_shared_daemon_state(Config::new());
    let snap = build_property_snapshot(&ds).await;
    assert_eq!(snap.playback, PlaybackStatus::Stopped);
}

#[tokio::test]
#[serial]
async fn can_go_next_true_when_not_at_end() {
    let ds = new_shared_daemon_state(Config::new());
    {
        let mut s = ds.write().await;
        s.queue = vec![song("a", "A"), song("b", "B"), song("c", "C")];
        s.queue_position = Some(0);
    }
    let snap = build_property_snapshot(&ds).await;
    assert!(snap.can_go_next);
    assert!(!snap.can_go_prev);
}

#[tokio::test]
#[serial]
async fn can_go_previous_true_when_not_at_start() {
    let ds = new_shared_daemon_state(Config::new());
    {
        let mut s = ds.write().await;
        s.queue = vec![song("a", "A"), song("b", "B")];
        s.queue_position = Some(1);
    }
    let snap = build_property_snapshot(&ds).await;
    assert!(snap.can_go_prev);
    assert!(!snap.can_go_next);
}

#[tokio::test]
#[serial]
async fn no_queue_position_means_neither_direction_available() {
    let ds = new_shared_daemon_state(Config::new());
    let snap = build_property_snapshot(&ds).await;
    assert!(!snap.can_go_next);
    assert!(!snap.can_go_prev);
}

#[tokio::test]
#[serial]
async fn empty_state_yields_no_metadata() {
    let ds = new_shared_daemon_state(Config::new());
    let snap = build_property_snapshot(&ds).await;
    assert!(snap.metadata.is_none());
}

#[tokio::test]
#[serial]
async fn metadata_populated_when_current_song_set() {
    let ds = new_shared_daemon_state(Config::new());
    {
        let mut s = ds.write().await;
        let sng = song("track-1", "Pictures of You");
        s.queue.push(sng.clone());
        s.queue_position = Some(0);
        s.now_playing.song = Some(sng);
    }
    let snap = build_property_snapshot(&ds).await;
    let md = snap.metadata.expect("metadata should be Some");
    assert_eq!(
        md.title().map(String::from).as_deref(),
        Some("Pictures of You")
    );
}

#[tokio::test]
#[serial]
async fn metadata_includes_length_in_microseconds() {
    let ds = new_shared_daemon_state(Config::new());
    {
        let mut s = ds.write().await;
        let mut sng = song("a", "Track");
        sng.duration = Some(180);
        s.queue.push(sng.clone());
        s.queue_position = Some(0);
        s.now_playing.song = Some(sng);
    }
    let snap = build_property_snapshot(&ds).await;
    let md = snap.metadata.unwrap();
    let len = md.length().unwrap();
    assert_eq!(len.as_micros(), 180 * 1_000_000);
}

#[tokio::test]
#[serial]
async fn metadata_includes_art_url_when_cover_art_present() {
    let mut cfg = Config::new();
    cfg.base_url = "https://example.com".into();
    cfg.username = "u".into();
    cfg.password = "p".into();
    let ds = new_shared_daemon_state(cfg);
    {
        let mut s = ds.write().await;
        let mut sng = song("a", "Track");
        sng.cover_art = Some("art-99".into());
        s.queue.push(sng.clone());
        s.queue_position = Some(0);
        s.now_playing.song = Some(sng);
    }
    let snap = build_property_snapshot(&ds).await;
    let md = snap.metadata.unwrap();
    let art = md.art_url();
    assert!(
        art.is_some(),
        "art_url should be set when cover_art id present"
    );
    let art = art.unwrap();
    assert!(art.contains("id=art-99"));
    assert!(art.contains("/rest/getCoverArt"));
}

#[tokio::test]
#[serial]
async fn metadata_omits_art_url_when_no_base_url() {
    let ds = new_shared_daemon_state(Config::new());
    {
        let mut s = ds.write().await;
        let mut sng = song("a", "Track");
        sng.cover_art = Some("art-1".into());
        s.queue.push(sng.clone());
        s.queue_position = Some(0);
        s.now_playing.song = Some(sng);
    }
    let snap = build_property_snapshot(&ds).await;
    let md = snap.metadata.unwrap();
    assert!(md.art_url().is_none());
}

#[tokio::test]
#[serial]
async fn metadata_includes_artist_and_album_when_set() {
    let ds = new_shared_daemon_state(Config::new());
    {
        let mut s = ds.write().await;
        let mut sng = song("a", "Title");
        sng.artist = Some("Joy Division".into());
        sng.album = Some("Closer".into());
        s.queue.push(sng.clone());
        s.queue_position = Some(0);
        s.now_playing.song = Some(sng);
    }
    let snap = build_property_snapshot(&ds).await;
    let md = snap.metadata.unwrap();
    let artists = md.artist().map(|v| v.to_vec()).unwrap_or_default();
    assert!(artists.iter().any(|a| a == "Joy Division"));
    assert_eq!(md.album().map(String::from).as_deref(), Some("Closer"));
}

#[tokio::test]
#[serial]
async fn last_position_in_queue_has_no_next_but_has_prev() {
    let ds = new_shared_daemon_state(Config::new());
    {
        let mut s = ds.write().await;
        s.queue = vec![song("a", "A"), song("b", "B"), song("c", "C")];
        s.queue_position = Some(2);
    }
    let snap = build_property_snapshot(&ds).await;
    assert!(!snap.can_go_next);
    assert!(snap.can_go_prev);
}
