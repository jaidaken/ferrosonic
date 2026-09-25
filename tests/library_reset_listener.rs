//! In-process TUI listener for library resets: reloads what the user has open,
//! and stops with the daemon.

mod common;

use std::sync::Arc;
use std::time::Duration;

use common::TestDaemon;
use ferrosonic::app::event_pump::spawn_library_reset_listener;
use ferrosonic::app::App;
use ferrosonic::ipc::client::DaemonClient;
use ferrosonic::ipc::InProcessClient;
use serial_test::serial;

#[tokio::test]
#[serial]
async fn library_reset_reloads_the_expanded_artist_in_process() {
    let td = TestDaemon::new().await;
    td.fake_subsonic
        .expect_get_artist("ar-1", "Artist", &["First Album"])
        .await;
    let client: Arc<dyn DaemonClient> = Arc::new(InProcessClient::new(td.core.clone()));
    let cfg = td.state.read().await.config.clone();
    let app = App::with_remote_client(client.clone(), cfg);
    app.client_state
        .write()
        .await
        .artists
        .expanded
        .insert("ar-1".into());
    let listener = spawn_library_reset_listener(
        td.core.clone(),
        app.daemon_state.clone(),
        app.client_state.clone(),
        client,
    );

    td.core.invalidate_library_caches().await;

    let mut names: Vec<String> = Vec::new();
    for _ in 0..200 {
        if let Some(albums) = app
            .daemon_state
            .read()
            .await
            .library
            .albums_cache
            .get("ar-1")
        {
            names = albums.iter().map(|a| a.name.clone()).collect();
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(names, ["First Album"], "the reset reloads the open artist");

    td.core.request_shutdown();
    tokio::time::timeout(Duration::from_secs(5), listener)
        .await
        .expect("listener stops on shutdown")
        .expect("listener task joins");
}

#[tokio::test]
#[serial]
async fn library_reset_listener_stops_when_the_daemon_shuts_down() {
    let td = TestDaemon::new().await;
    let client: Arc<dyn DaemonClient> = Arc::new(InProcessClient::new(td.core.clone()));
    let cfg = td.state.read().await.config.clone();
    let app = App::with_remote_client(client.clone(), cfg);
    let listener = spawn_library_reset_listener(
        td.core.clone(),
        app.daemon_state.clone(),
        app.client_state.clone(),
        client,
    );

    td.core.request_shutdown();

    let joined = tokio::time::timeout(Duration::from_secs(5), listener).await;
    assert!(
        joined.is_ok(),
        "the listener must end with the daemon, not wait on an event channel that stays open"
    );
}
