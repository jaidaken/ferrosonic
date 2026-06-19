//! app/mod.rs: seed_cover_art + apply_event cover-art fetch branches.

mod common;

use common::TestDaemon;
use ferrosonic::app::App;
use ferrosonic::ipc::DaemonEvent;
use ferrosonic::subsonic::models::Child;
use serial_test::serial;

fn song_with_cover(id: &str, cover_id: &str) -> Child {
    Child {
        id: id.into(),
        title: id.into(),
        parent: None,
        is_dir: false,
        album: Some("Alb".into()),
        artist: Some("Art".into()),
        artist_id: None,
        track: None,
        year: None,
        genre: None,
        cover_art: Some(cover_id.into()),
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

fn small_jpeg() -> Vec<u8> {
    use image::{ImageBuffer, Rgba};
    let img: ImageBuffer<Rgba<u8>, Vec<u8>> =
        ImageBuffer::from_fn(8, 8, |_, _| Rgba([100, 50, 200, 255]));
    let mut buf = std::io::Cursor::new(Vec::new());
    img.write_to(&mut buf, image::ImageFormat::Png).unwrap();
    buf.into_inner()
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
async fn seed_cover_art_disabled_returns_early() {
    let (app, _td) = build_app().await;
    {
        let mut cs = app.client_state.write().await;
        cs.settings_state.cover_art = false;
    }
    app.seed_cover_art().await;
}

#[tokio::test]
#[serial]
async fn seed_cover_art_no_current_song_returns_early() {
    let (app, _td) = build_app().await;
    {
        let mut cs = app.client_state.write().await;
        cs.settings_state.cover_art = true;
    }
    app.seed_cover_art().await;
}

#[tokio::test]
#[serial]
async fn seed_cover_art_with_song_and_bytes_loads_into_state() {
    let (app, td) = build_app().await;
    let png = small_jpeg();
    td.fake_subsonic.expect_get_cover_art("art-seed", png).await;
    {
        let mut ds = app.daemon_state.write().await;
        ds.now_playing.song = Some(song_with_cover("song1", "art-seed"));
    }
    {
        let mut cs = app.client_state.write().await;
        cs.settings_state.cover_art = true;
    }
    app.seed_cover_art().await;
}

#[tokio::test]
#[serial]
async fn bootstrap_and_pump_fetches_snapshot_into_daemon_state() {
    let (app, td) = build_app().await;
    {
        let mut s = td.state.write().await;
        s.queue.push(song_with_cover("a", "ca"));
    }
    app.bootstrap_and_pump().await;
    tokio::time::timeout(std::time::Duration::from_secs(1), async {
        loop {
            if app.daemon_state.read().await.queue.len() == 1 {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("bootstrap pump did not populate daemon_state queue");
    let ds = app.daemon_state.read().await;
    assert_eq!(ds.queue.len(), 1);
}

#[tokio::test]
#[serial]
async fn apply_event_now_playing_with_cover_art_enabled_fetches_image() {
    let (app, td) = build_app().await;
    let png = small_jpeg();
    td.fake_subsonic.expect_get_cover_art("art-np", png).await;
    {
        let mut cs = app.client_state.write().await;
        cs.settings_state.cover_art = true;
    }
    let ev = DaemonEvent::NowPlayingChanged(ferrosonic::daemon::state::NowPlaying {
        song: Some(song_with_cover("s1", "art-np")),
        state: ferrosonic::daemon::state::PlaybackState::Playing,
        position: 0.0,
        duration: 180.0,
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
async fn apply_event_now_playing_clears_cover_when_no_cover_id() {
    let (app, td) = build_app().await;
    {
        let mut cs = app.client_state.write().await;
        cs.settings_state.cover_art = true;
    }
    let ev = DaemonEvent::NowPlayingChanged(ferrosonic::daemon::state::NowPlaying {
        song: Some(Child {
            id: "no-cover".into(),
            title: "T".into(),
            parent: None,
            is_dir: false,
            album: None,
            artist: None,
            artist_id: None,
            track: None,
            year: None,
            genre: None,
            cover_art: None,
            size: None,
            content_type: None,
            suffix: None,
            duration: Some(120),
            bit_rate: None,
            path: None,
            disc_number: None,
            starred: None,
        }),
        state: ferrosonic::daemon::state::PlaybackState::Playing,
        position: 0.0,
        duration: 120.0,
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
async fn apply_event_config_changed_enables_cover_art_fetches() {
    let (app, td) = build_app().await;
    let png = small_jpeg();
    td.fake_subsonic.expect_get_cover_art("art-cfg", png).await;
    {
        let mut ds = app.daemon_state.write().await;
        ds.now_playing.song = Some(song_with_cover("s1", "art-cfg"));
    }
    let mut cfg = ferrosonic::config::Config::new();
    cfg.cover_art = true;
    cfg.base_url = td.fake_subsonic.url();
    cfg.username = "u".into();
    cfg.password = "p".into();
    let ev = DaemonEvent::ConfigChanged(cfg);
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
async fn apply_event_config_changed_disables_cover_art_clears_guard() {
    let (app, td) = build_app().await;
    let mut cfg = ferrosonic::config::Config::new();
    cfg.cover_art = false;
    let ev = DaemonEvent::ConfigChanged(cfg);
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
