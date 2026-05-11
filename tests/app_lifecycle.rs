//! App lifecycle: seed_cover_art, bootstrap_and_pump.

use std::sync::Arc;

mod common;

use common::RecordingClient;
use ferrosonic::app::App;
use ferrosonic::config::Config;
use serial_test::serial;

struct AppFixture {
    app: App,
    _tempdir: tempfile::TempDir,
}

async fn build_app_remote(client: Arc<dyn ferrosonic::ipc::client::DaemonClient>) -> AppFixture {
    let tempdir = tempfile::tempdir().unwrap();
    std::env::set_var("FERROSONIC_CONFIG_DIR", tempdir.path());
    let mut config = Config::new();
    config.daemon = true;
    let app = App::with_remote_client(client, config);
    AppFixture {
        app,
        _tempdir: tempdir,
    }
}

#[tokio::test]
#[serial]
async fn seed_cover_art_with_no_song_is_noop() {
    let rec = RecordingClient::new();
    let fx = build_app_remote(rec.clone() as Arc<dyn ferrosonic::ipc::client::DaemonClient>).await;
    fx.app.seed_cover_art().await;
    assert!(
        rec.requests().await.is_empty(),
        "no current song; seed_cover_art must not request anything"
    );
}

#[tokio::test]
#[serial]
async fn seed_cover_art_with_cover_art_disabled_is_noop() {
    let rec = RecordingClient::new();
    let fx = build_app_remote(rec.clone() as Arc<dyn ferrosonic::ipc::client::DaemonClient>).await;
    {
        let mut ds = fx.app.daemon_state.write().await;
        ds.now_playing.song = Some(common::song("a", "Track"));
        ds.now_playing.song.as_mut().unwrap().cover_art = Some("art-1".into());
    }
    {
        let mut cs = fx.app.client_state.write().await;
        cs.settings_state.cover_art = false;
    }
    fx.app.seed_cover_art().await;
    assert!(
        rec.requests().await.is_empty(),
        "cover_art setting off; seed_cover_art must not request"
    );
}

#[tokio::test]
#[serial]
async fn seed_cover_art_with_song_and_setting_on_requests_fetch() {
    let rec = RecordingClient::new();
    let fx = build_app_remote(rec.clone() as Arc<dyn ferrosonic::ipc::client::DaemonClient>).await;
    {
        let mut ds = fx.app.daemon_state.write().await;
        let mut song = common::song("a", "Track");
        song.cover_art = Some("art-1".into());
        ds.now_playing.song = Some(song);
    }
    {
        let mut cs = fx.app.client_state.write().await;
        cs.settings_state.cover_art = true;
    }
    fx.app.seed_cover_art().await;
    let reqs = rec.requests().await;
    assert!(
        reqs.iter().any(|r| matches!(
            r,
            ferrosonic::ipc::protocol::DaemonRequest::FetchCoverArt { .. }
        )),
        "expected FetchCoverArt request; got {:?}",
        reqs
    );
}

#[tokio::test]
#[serial]
async fn bootstrap_and_pump_subscribes_and_snapshots() {
    let rec = RecordingClient::new();
    let fx = build_app_remote(rec.clone() as Arc<dyn ferrosonic::ipc::client::DaemonClient>).await;
    tokio::time::timeout(
        std::time::Duration::from_millis(300),
        fx.app.bootstrap_and_pump(),
    )
    .await
    .ok();
    let reqs = rec.requests().await;
    assert!(
        reqs.iter()
            .any(|r| matches!(r, ferrosonic::ipc::protocol::DaemonRequest::Snapshot)),
        "bootstrap_and_pump should request Snapshot; got {:?}",
        reqs
    );
}
