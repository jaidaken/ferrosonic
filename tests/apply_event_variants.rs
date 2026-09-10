//! `apply_event` dispatch coverage for each DaemonEvent variant.

mod common;

use std::sync::Arc;

use common::{song, RecordingClient};
use ferrosonic::app::apply_event;
use ferrosonic::app::state::{new_shared_client_state, new_shared_daemon_state};
use ferrosonic::config::{Config, RepeatMode};
use ferrosonic::ipc::client::DaemonClient;
use ferrosonic::ipc::protocol::QuickPlayAlbumKind;
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
    let tempdir = common::tempdir();
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
async fn queue_changed_clamps_a_stale_client_cursor() {
    let h = build_harness();
    h.client_state.write().await.queue_state.selected = Some(9);
    apply_event(
        &h.daemon,
        &h.client_state,
        &h.client,
        &h.cover_art,
        DaemonEvent::QueueChanged {
            queue: vec![song("a", "A"), song("b", "B")],
            position: Some(0),
        },
    )
    .await;
    assert_eq!(
        h.client_state.read().await.queue_state.selected,
        Some(1),
        "a cursor past the end must clamp to the last queue row"
    );

    apply_event(
        &h.daemon,
        &h.client_state,
        &h.client,
        &h.cover_art,
        DaemonEvent::QueueChanged {
            queue: Vec::new(),
            position: None,
        },
    )
    .await;
    assert_eq!(
        h.client_state.read().await.queue_state.selected,
        None,
        "an emptied queue must clear the cursor"
    );
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
    use ferrosonic::daemon::state::{NowPlaying, PlaybackState};
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
        DaemonEvent::NowPlayingChanged(Box::new(np)),
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
        ds.library
            .quick_play_album_songs
            .insert(QuickPlayAlbumKind::Newest, vec![song("hit", "Hit")]);
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
    assert!(
        ds.library.quick_play_album_songs[&QuickPlayAlbumKind::Newest][0]
            .starred
            .is_some()
    );
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
async fn quick_play_album_event_replaces_and_clears_its_category() {
    let h = build_harness();
    for songs in [vec![song("new", "New")], Vec::new()] {
        apply_event(
            &h.daemon,
            &h.client_state,
            &h.client,
            &h.cover_art,
            DaemonEvent::QuickPlayAlbumChanged {
                kind: QuickPlayAlbumKind::Newest,
                songs,
            },
        )
        .await;
    }
    assert!(!h
        .daemon
        .read()
        .await
        .library
        .quick_play_album_songs
        .contains_key(&QuickPlayAlbumKind::Newest));
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
        original_release_date: None,
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
        DaemonEvent::ConfigChanged(Box::new(cfg)),
    )
    .await;
    let ds = h.daemon.read().await;
    assert_eq!(ds.config.theme, "dracula");
    assert!(ds.config.cava);
}

#[tokio::test]
#[serial]
async fn config_changed_event_mirrors_cava_and_daemon_settings() {
    let h = build_harness();
    let mut cfg = Config::new();
    cfg.cava = true;
    cfg.cava_size = 42;
    cfg.daemon = true;
    apply_event(
        &h.daemon,
        &h.client_state,
        &h.client,
        &h.cover_art,
        DaemonEvent::ConfigChanged(Box::new(cfg)),
    )
    .await;
    let cs = h.client_state.read().await;
    assert!(
        cs.settings_state.cava_enabled,
        "cava toggle must mirror into the Settings page state"
    );
    assert_eq!(cs.settings_state.cava_size, 42);
    assert!(
        cs.settings_state.daemon_enabled,
        "daemon toggle must mirror into the Settings page state"
    );
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

#[tokio::test]
#[serial]
async fn rating_events_update_all_client_copies_and_clear() {
    let h = build_harness();
    let list = vec![
        song("target", "One"),
        song("other", "Other"),
        song("target", "Duplicate"),
    ];
    {
        let mut state = h.daemon.write().await;
        state.queue = list.clone();
        state.library.starred_songs = list.clone();
        state.library.random_songs = list.clone();
        state.library.random_album_songs = list.clone();
        state
            .library
            .quick_play_album_songs
            .insert(QuickPlayAlbumKind::Newest, list.clone());
        state
            .library
            .album_songs_cache
            .insert("album".into(), list.clone());
        state
            .library
            .playlist_songs_cache
            .insert("playlist".into(), list.clone());
        state.now_playing.song = Some(list[0].clone());
    }
    {
        let mut client = h.client_state.write().await;
        client.artists.songs = list.clone();
        client.playlists.songs = list;
    }
    for rating in [Some(4), None] {
        apply_event(
            &h.daemon,
            &h.client_state,
            &h.client,
            &h.cover_art,
            DaemonEvent::SongRatingChanged {
                id: "target".into(),
                rating,
            },
        )
        .await;
        let state = h.daemon.read().await;
        let client = h.client_state.read().await;
        for list in [
            &state.queue,
            &state.library.starred_songs,
            &state.library.random_songs,
            &state.library.random_album_songs,
            &state.library.quick_play_album_songs[&QuickPlayAlbumKind::Newest],
            &state.library.album_songs_cache["album"],
            &state.library.playlist_songs_cache["playlist"],
            &client.artists.songs,
            &client.playlists.songs,
        ] {
            assert_eq!(list[0].user_rating, rating);
            assert_eq!(list[1].user_rating, None);
            assert_eq!(list[2].user_rating, rating);
        }
        assert_eq!(state.now_playing.song.as_ref().unwrap().user_rating, rating);
    }
}

#[tokio::test]
#[serial]
async fn config_event_synchronizes_new_settings_without_purging_queue() {
    use ferrosonic::config::{PlaybackFilters, ReplayGainMode};
    let h = build_harness();
    let mut config = Config::new();
    config.cover_art = false;
    config.replay_gain_mode = ReplayGainMode::Album;
    config.replay_gain_preamp = 2.5;
    config.replay_gain_clip = true;
    config.playback_filters = PlaybackFilters {
        min_rating: 5,
        ..Default::default()
    };
    let mut queued = song("low", "Low rated");
    queued.user_rating = Some(1);
    h.daemon.write().await.queue = vec![queued];
    apply_event(
        &h.daemon,
        &h.client_state,
        &h.client,
        &h.cover_art,
        DaemonEvent::ConfigChanged(Box::new(config)),
    )
    .await;
    let state = h.daemon.read().await;
    let client = h.client_state.read().await;
    assert_eq!(
        client.settings_state.replay_gain_mode,
        ReplayGainMode::Album
    );
    assert_eq!(client.settings_state.replay_gain_preamp, 2.5);
    assert!(client.settings_state.replay_gain_clip);
    assert_eq!(client.settings_state.playback_filters.min_rating, 5);
    assert_eq!(state.config.playback_filters.min_rating, 5);
    assert_eq!(state.queue[0].id, "low");
}

/// A rating (or star) applied to a song shown in the Library tree's search
/// results must update that list too. `build_tree_items` renders song rows
/// straight out of `artists.search_results` whenever a filter is active, so
/// a song rated from there stays visually unrated until the search is
/// re-run, even though the write reached the server.
#[tokio::test]
#[serial]
async fn rating_updates_a_song_shown_in_library_search_results() {
    let h = build_harness();
    {
        let mut client = h.client_state.write().await;
        client.artists.filter = "target".into();
        let mut results = ferrosonic::subsonic::models::SearchResult3::default();
        results.song = vec![song("target", "Target"), song("other", "Other")];
        client.artists.search_results = Some(results);
    }
    apply_event(
        &h.daemon,
        &h.client_state,
        &h.client,
        &h.cover_art,
        DaemonEvent::SongRatingChanged {
            id: "target".into(),
            rating: Some(5),
        },
    )
    .await;
    let client = h.client_state.read().await;
    let results = client.artists.search_results.as_ref().unwrap();
    assert_eq!(
        results.song[0].user_rating,
        Some(5),
        "the rated search-result row must show its new rating"
    );
    assert_eq!(results.song[1].user_rating, None);
}

/// The same gap for stars, which share the update path.
#[tokio::test]
#[serial]
async fn starring_updates_a_song_shown_in_library_search_results() {
    let h = build_harness();
    {
        let mut client = h.client_state.write().await;
        client.artists.filter = "target".into();
        let mut results = ferrosonic::subsonic::models::SearchResult3::default();
        results.song = vec![song("target", "Target")];
        client.artists.search_results = Some(results);
    }
    apply_event(
        &h.daemon,
        &h.client_state,
        &h.client,
        &h.cover_art,
        DaemonEvent::SongStarChanged {
            id: "target".into(),
            starred: true,
        },
    )
    .await;
    let client = h.client_state.read().await;
    assert!(
        client.artists.search_results.as_ref().unwrap().song[0]
            .starred
            .is_some(),
        "the starred search-result row must show its star"
    );
}
