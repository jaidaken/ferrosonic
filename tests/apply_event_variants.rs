//! `apply_event` dispatch coverage for each DaemonEvent variant.

mod common;

use std::sync::Arc;

use common::{song, RecordingClient};
use ferrosonic::app::apply_event;
use ferrosonic::app::state::{new_shared_client_state, new_shared_daemon_state};
use ferrosonic::config::{Config, RepeatMode};
use ferrosonic::ipc::client::DaemonClient;
use ferrosonic::ipc::DaemonEvent;
use ferrosonic::subsonic::models::{Album, Artist, Playlist};
use ferrosonic::ui::cover_art::CoverArtState;
use serial_test::serial;

struct Harness {
    daemon: ferrosonic::app::state::SharedDaemonState,
    client_state: ferrosonic::app::state::SharedClientState,
    client: Arc<dyn DaemonClient>,
    cover_art: std::sync::Arc<std::sync::Mutex<CoverArtState>>,
    _tempdir: tempfile::TempDir,
}

fn build_harness() -> Harness {
    let tempdir = tempfile::tempdir().unwrap();
    std::env::set_var("FERROSONIC_CONFIG_DIR", tempdir.path());
    let config = Config::new();
    let daemon = new_shared_daemon_state(config.clone());
    let client_state = new_shared_client_state(&config);
    let client: Arc<dyn DaemonClient> = RecordingClient::new();
    let cover_art = std::sync::Arc::new(std::sync::Mutex::new(CoverArtState {
        picker: None,
        protocol_type: None,
        cell_size: (8, 16),
        current_id: None,
        image: None,
        protocol: None,
        chafa_cache: None,
    }));
    Harness {
        daemon,
        client_state,
        client,
        cover_art,
        _tempdir: tempdir,
    }
}

#[tokio::test]
#[serial]
async fn queue_changed_event_updates_queue_and_position() {
    let h = build_harness();
    let ev = DaemonEvent::QueueChanged {
        queue: vec![song("a", "A"), song("b", "B")],
        position: Some(1),
    };
    apply_event(&h.daemon, &h.client_state, &h.client, &h.cover_art, ev).await;
    let ds = h.daemon.read().await;
    assert_eq!(ds.queue.len(), 2);
    assert_eq!(ds.queue_position, Some(1));
}

#[tokio::test]
#[serial]
async fn now_playing_changed_event_updates_state() {
    use ferrosonic::app::state::{NowPlaying, PlaybackState};
    let h = build_harness();
    let np = NowPlaying {
        song: Some(song("x", "X")),
        state: PlaybackState::Playing,
        duration: 200.0,
        ..Default::default()
    };
    apply_event(
        &h.daemon,
        &h.client_state,
        &h.client,
        &h.cover_art,
        DaemonEvent::NowPlayingChanged(np),
    )
    .await;
    let ds = h.daemon.read().await;
    assert_eq!(ds.now_playing.state, PlaybackState::Playing);
    assert_eq!(ds.now_playing.duration, 200.0);
}

#[tokio::test]
#[serial]
async fn position_tick_event_updates_position() {
    let h = build_harness();
    apply_event(
        &h.daemon,
        &h.client_state,
        &h.client,
        &h.cover_art,
        DaemonEvent::PositionTick(42.5),
    )
    .await;
    let ds = h.daemon.read().await;
    assert!((ds.now_playing.position - 42.5).abs() < 1e-9);
}

#[tokio::test]
#[serial]
async fn starred_changed_event_replaces_library_starred() {
    let h = build_harness();
    apply_event(
        &h.daemon,
        &h.client_state,
        &h.client,
        &h.cover_art,
        DaemonEvent::StarredChanged(vec![song("s0", "S0"), song("s1", "S1")]),
    )
    .await;
    let ds = h.daemon.read().await;
    assert_eq!(ds.library.starred_songs.len(), 2);
}

#[tokio::test]
#[serial]
async fn song_star_changed_event_updates_all_lists() {
    let h = build_harness();
    {
        let mut ds = h.daemon.write().await;
        ds.queue.push(song("hit", "Hit"));
        ds.library.random_songs.push(song("hit", "Hit"));
    }
    apply_event(
        &h.daemon,
        &h.client_state,
        &h.client,
        &h.cover_art,
        DaemonEvent::SongStarChanged {
            id: "hit".into(),
            starred: true,
        },
    )
    .await;
    let ds = h.daemon.read().await;
    assert!(ds.queue[0].starred.is_some());
    assert!(ds.library.random_songs[0].starred.is_some());
}

#[tokio::test]
#[serial]
async fn random_changed_event_replaces_library_random() {
    let h = build_harness();
    apply_event(
        &h.daemon,
        &h.client_state,
        &h.client,
        &h.cover_art,
        DaemonEvent::RandomChanged(vec![song("r0", "R0")]),
    )
    .await;
    let ds = h.daemon.read().await;
    assert_eq!(ds.library.random_songs.len(), 1);
}

#[tokio::test]
#[serial]
async fn artists_changed_event_replaces_artists() {
    let h = build_harness();
    let artists = vec![Artist {
        id: "a0".into(),
        name: "Artist".into(),
        album_count: Some(1),
        cover_art: None,
    }];
    apply_event(
        &h.daemon,
        &h.client_state,
        &h.client,
        &h.cover_art,
        DaemonEvent::ArtistsChanged(artists),
    )
    .await;
    let ds = h.daemon.read().await;
    assert_eq!(ds.library.artists.len(), 1);
}

#[tokio::test]
#[serial]
async fn albums_changed_event_inserts_into_cache() {
    let h = build_harness();
    let albums = vec![Album {
        id: "alb0".into(),
        name: "Album".into(),
        artist: Some("X".into()),
        artist_id: Some("a0".into()),
        cover_art: None,
        song_count: Some(5),
        duration: Some(900),
        year: None,
        genre: None,
    }];
    apply_event(
        &h.daemon,
        &h.client_state,
        &h.client,
        &h.cover_art,
        DaemonEvent::AlbumsChanged {
            artist_id: "a0".into(),
            albums,
        },
    )
    .await;
    let ds = h.daemon.read().await;
    assert!(ds.library.albums_cache.contains_key("a0"));
}

#[tokio::test]
#[serial]
async fn album_songs_changed_event_inserts_into_cache() {
    let h = build_harness();
    apply_event(
        &h.daemon,
        &h.client_state,
        &h.client,
        &h.cover_art,
        DaemonEvent::AlbumSongsChanged {
            album_id: "alb0".into(),
            songs: vec![song("s0", "S0"), song("s1", "S1")],
        },
    )
    .await;
    let ds = h.daemon.read().await;
    assert!(ds.library.album_songs_cache.contains_key("alb0"));
}

#[tokio::test]
#[serial]
async fn playlists_changed_event_replaces_playlists() {
    let h = build_harness();
    let pls = vec![Playlist {
        id: "p0".into(),
        name: "P".into(),
        owner: None,
        song_count: Some(3),
        duration: Some(540),
        cover_art: None,
        public: None,
        comment: None,
    }];
    apply_event(
        &h.daemon,
        &h.client_state,
        &h.client,
        &h.cover_art,
        DaemonEvent::PlaylistsChanged(pls),
    )
    .await;
    let ds = h.daemon.read().await;
    assert_eq!(ds.library.playlists.len(), 1);
}

#[tokio::test]
#[serial]
async fn playlist_songs_changed_event_inserts_into_cache() {
    let h = build_harness();
    apply_event(
        &h.daemon,
        &h.client_state,
        &h.client,
        &h.cover_art,
        DaemonEvent::PlaylistSongsChanged {
            playlist_id: "p0".into(),
            songs: vec![song("s0", "S0")],
        },
    )
    .await;
    let ds = h.daemon.read().await;
    assert!(ds.library.playlist_songs_cache.contains_key("p0"));
}

#[tokio::test]
#[serial]
async fn notification_event_sets_client_notification() {
    let h = build_harness();
    apply_event(
        &h.daemon,
        &h.client_state,
        &h.client,
        &h.cover_art,
        DaemonEvent::Notification {
            message: "hello".into(),
            is_error: false,
        },
    )
    .await;
    let cs = h.client_state.read().await;
    assert!(cs.notification.is_some());
    assert_eq!(cs.notification.as_ref().unwrap().message, "hello");
}

#[tokio::test]
#[serial]
async fn notification_error_event_marks_is_error() {
    let h = build_harness();
    apply_event(
        &h.daemon,
        &h.client_state,
        &h.client,
        &h.cover_art,
        DaemonEvent::Notification {
            message: "oops".into(),
            is_error: true,
        },
    )
    .await;
    let cs = h.client_state.read().await;
    assert!(cs.notification.as_ref().unwrap().is_error);
}

#[tokio::test]
#[serial]
async fn config_changed_event_overwrites_daemon_config() {
    let h = build_harness();
    let mut cfg = Config::new();
    cfg.theme = "dracula".into();
    cfg.cava = true;
    apply_event(
        &h.daemon,
        &h.client_state,
        &h.client,
        &h.cover_art,
        DaemonEvent::ConfigChanged(cfg),
    )
    .await;
    let ds = h.daemon.read().await;
    assert_eq!(ds.config.theme, "dracula");
    assert!(ds.config.cava);
}

#[tokio::test]
#[serial]
async fn repeat_mode_changed_event_updates_client_state() {
    let h = build_harness();
    apply_event(
        &h.daemon,
        &h.client_state,
        &h.client,
        &h.cover_art,
        DaemonEvent::RepeatModeChanged(RepeatMode::All),
    )
    .await;
    let cs = h.client_state.read().await;
    assert_eq!(cs.settings_state.repeat_mode, RepeatMode::All);
}

#[tokio::test]
#[serial]
async fn shutdown_event_sets_should_quit() {
    let h = build_harness();
    apply_event(
        &h.daemon,
        &h.client_state,
        &h.client,
        &h.cover_art,
        DaemonEvent::Shutdown,
    )
    .await;
    let cs = h.client_state.read().await;
    assert!(cs.should_quit);
}
