//! apply_event SongStarChanged with populated state lists.

mod common;

use common::TestDaemon;
use ferrosonic::app::App;
use ferrosonic::ipc::DaemonEvent;
use ferrosonic::subsonic::models::Child;
use serial_test::serial;

fn song(id: &str) -> Child {
    Child {
        id: id.into(),
        title: id.into(),
        parent: None,
        is_dir: false,
        album: None,
        artist: None,
        track: None,
        year: None,
        genre: None,
        cover_art: None,
        size: None,
        content_type: None,
        suffix: None,
        duration: Some(180),
        bit_rate: None,
        path: None,
        disc_number: None,
        starred: None,
    }
}

async fn build_app() -> (App, TestDaemon) {
    let td = TestDaemon::new().await;
    let cfg = td.state.read().await.config.clone();
    let app = App::with_remote_client(
        std::sync::Arc::new(ferrosonic::ipc::InProcessClient::new(td.core.clone())),
        cfg,
    );
    (app, td)
}

#[tokio::test]
#[serial]
async fn song_star_changed_updates_queue_and_random_and_caches_and_np() {
    let (app, td) = build_app().await;
    {
        let mut s = app.daemon_state.write().await;
        s.queue.push(song("target"));
        s.library.random_songs.push(song("target"));
        s.library
            .album_songs_cache
            .insert("alb".into(), vec![song("target")]);
        s.library
            .playlist_songs_cache
            .insert("pl".into(), vec![song("target")]);
        s.now_playing.song = Some(song("target"));
    }
    {
        let mut cs = app.client_state.write().await;
        cs.artists.songs.push(song("target"));
        cs.playlists.songs.push(song("target"));
    }

    let ev = DaemonEvent::SongStarChanged {
        id: "target".into(),
        starred: true,
    };
    let client: std::sync::Arc<dyn ferrosonic::ipc::DaemonClient> =
        std::sync::Arc::new(ferrosonic::ipc::InProcessClient::new(td.core.clone()));
    let cover = std::sync::Arc::new(std::sync::Mutex::new(
        ferrosonic::ui::cover_art::CoverArtState {
            picker: Some(ratatui_image::picker::Picker::from_fontsize((8, 16))),
            protocol_type: Some(ratatui_image::picker::ProtocolType::Halfblocks),
            cell_size: (8, 16),
            current_id: None,
            image: None,
            protocol: None,
            chafa_cache: None,
        },
    ));
    ferrosonic::app::apply_event(&app.daemon_state, &app.client_state, &client, &cover, ev).await;

    let ds = app.daemon_state.read().await;
    assert!(ds.queue[0].starred.is_some());
    assert!(ds.library.random_songs[0].starred.is_some());
    assert!(ds.library.album_songs_cache["alb"][0].starred.is_some());
    assert!(ds.library.playlist_songs_cache["pl"][0].starred.is_some());
    assert!(ds.now_playing.song.as_ref().unwrap().starred.is_some());

    let cs = app.client_state.read().await;
    assert!(cs.artists.songs[0].starred.is_some());
    assert!(cs.playlists.songs[0].starred.is_some());
}

#[tokio::test]
#[serial]
async fn song_star_changed_unstar_clears_marker_everywhere() {
    let (app, td) = build_app().await;
    let mut starred = song("target");
    starred.starred = Some("now".into());
    {
        let mut s = app.daemon_state.write().await;
        s.queue.push(starred.clone());
        s.library.random_songs.push(starred.clone());
        s.now_playing.song = Some(starred.clone());
    }

    let ev = DaemonEvent::SongStarChanged {
        id: "target".into(),
        starred: false,
    };
    let client: std::sync::Arc<dyn ferrosonic::ipc::DaemonClient> =
        std::sync::Arc::new(ferrosonic::ipc::InProcessClient::new(td.core.clone()));
    let cover = std::sync::Arc::new(std::sync::Mutex::new(
        ferrosonic::ui::cover_art::CoverArtState {
            picker: Some(ratatui_image::picker::Picker::from_fontsize((8, 16))),
            protocol_type: Some(ratatui_image::picker::ProtocolType::Halfblocks),
            cell_size: (8, 16),
            current_id: None,
            image: None,
            protocol: None,
            chafa_cache: None,
        },
    ));
    ferrosonic::app::apply_event(&app.daemon_state, &app.client_state, &client, &cover, ev).await;

    let ds = app.daemon_state.read().await;
    assert!(ds.queue[0].starred.is_none());
    assert!(ds.library.random_songs[0].starred.is_none());
    assert!(ds.now_playing.song.as_ref().unwrap().starred.is_none());
}

#[tokio::test]
#[serial]
async fn apply_event_now_playing_when_disabled_skips_cover_fetch() {
    let (app, td) = build_app().await;
    {
        let mut cs = app.client_state.write().await;
        cs.settings_state.cover_art = false;
    }
    let ev = DaemonEvent::NowPlayingChanged(ferrosonic::app::state::NowPlaying {
        song: Some({
            let mut s = song("x");
            s.cover_art = Some("art".into());
            s
        }),
        state: ferrosonic::app::state::PlaybackState::Playing,
        position: 0.0,
        duration: 100.0,
        sample_rate: None,
        bit_depth: None,
        format: None,
        channels: None,
    });
    let client: std::sync::Arc<dyn ferrosonic::ipc::DaemonClient> =
        std::sync::Arc::new(ferrosonic::ipc::InProcessClient::new(td.core.clone()));
    let cover = std::sync::Arc::new(std::sync::Mutex::new(
        ferrosonic::ui::cover_art::CoverArtState {
            picker: Some(ratatui_image::picker::Picker::from_fontsize((8, 16))),
            protocol_type: Some(ratatui_image::picker::ProtocolType::Halfblocks),
            cell_size: (8, 16),
            current_id: None,
            image: None,
            protocol: None,
            chafa_cache: None,
        },
    ));
    ferrosonic::app::apply_event(&app.daemon_state, &app.client_state, &client, &cover, ev).await;
}

#[tokio::test]
#[serial]
async fn apply_event_now_playing_with_same_cover_id_skips_fetch() {
    let (app, td) = build_app().await;
    {
        let mut cs = app.client_state.write().await;
        cs.settings_state.cover_art = true;
    }
    let cover = std::sync::Arc::new(std::sync::Mutex::new(
        ferrosonic::ui::cover_art::CoverArtState {
            picker: Some(ratatui_image::picker::Picker::from_fontsize((8, 16))),
            protocol_type: Some(ratatui_image::picker::ProtocolType::Halfblocks),
            cell_size: (8, 16),
            current_id: Some("same-id".into()),
            image: None,
            protocol: None,
            chafa_cache: None,
        },
    ));
    let ev = DaemonEvent::NowPlayingChanged(ferrosonic::app::state::NowPlaying {
        song: Some({
            let mut s = song("x");
            s.cover_art = Some("same-id".into());
            s
        }),
        state: ferrosonic::app::state::PlaybackState::Playing,
        position: 0.0,
        duration: 100.0,
        sample_rate: None,
        bit_depth: None,
        format: None,
        channels: None,
    });
    let client: std::sync::Arc<dyn ferrosonic::ipc::DaemonClient> =
        std::sync::Arc::new(ferrosonic::ipc::InProcessClient::new(td.core.clone()));
    ferrosonic::app::apply_event(&app.daemon_state, &app.client_state, &client, &cover, ev).await;
}
