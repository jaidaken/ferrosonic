//! App::shutdown_subsystems: stop cava + quit mpv. Idempotent.

mod common;
use ferrosonic::app::App;
use ferrosonic::config::Config;
use serial_test::serial;

struct AppFixture {
    app: App,
    _tempdir: tempfile::TempDir,
}

async fn build_app(daemon_mode: bool) -> AppFixture {
    let tempdir = common::tempdir();
    std::env::set_var("FERROSONIC_CONFIG_DIR", tempdir.path());
    let mut config = Config::new();
    config.daemon = daemon_mode;
    let app = App::new(config);
    AppFixture {
        app,
        _tempdir: tempdir,
    }
}

#[tokio::test]
#[serial]
async fn shutdown_subsystems_on_fresh_app_is_safe() {
    let mut fx = build_app(false).await;
    fx.app.shutdown_subsystems().await;
}

#[tokio::test]
#[serial]
async fn shutdown_subsystems_is_idempotent() {
    let mut fx = build_app(false).await;
    fx.app.shutdown_subsystems().await;
    fx.app.shutdown_subsystems().await;
    fx.app.shutdown_subsystems().await;
}

#[tokio::test]
#[serial]
async fn shutdown_subsystems_works_in_remote_mode_without_core() {
    use std::sync::Arc;
    let tempdir = common::tempdir();
    std::env::set_var("FERROSONIC_CONFIG_DIR", tempdir.path());
    let mut config = Config::new();
    config.daemon = true;
    let core = ferrosonic::daemon::DaemonCore::new(
        ferrosonic::app::state::new_shared_daemon_state(config.clone()),
        &config,
    );
    let client = Arc::new(ferrosonic::ipc::client::InProcessClient::new(core));
    let mut app = App::with_remote_client(client, config);
    app.shutdown_subsystems().await;
}
