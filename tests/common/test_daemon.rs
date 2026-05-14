//! Real `DaemonCore` wired to fake mpv + fake Subsonic.
//!
//! Tests using `TestDaemon::new` set FERROSONIC_CONFIG_DIR; mark them
//! `#[serial_test::serial]` for cargo test (nextest is process-per-test).

use std::sync::Arc;

use tempfile::TempDir;

use ferrosonic::app::state::{new_shared_daemon_state, SharedDaemonState};
use ferrosonic::audio::mpv::MpvController;
use ferrosonic::config::Config;
use ferrosonic::daemon::DaemonCore;

use super::fake_mpv::FakeMpv;
use super::fake_subsonic::FakeSubsonic;

pub struct TestDaemon {
    pub core: Arc<DaemonCore>,
    pub state: SharedDaemonState,
    pub fake_mpv: FakeMpv,
    pub fake_subsonic: FakeSubsonic,
    pub config_dir: TempDir,
}

impl TestDaemon {
    pub async fn new() -> Self {
        let config_dir = tempfile::tempdir().expect("create config tempdir");
        Self::new_with_config_dir(config_dir).await
    }

    pub async fn new_with_config_dir(config_dir: TempDir) -> Self {
        std::env::set_var("FERROSONIC_CONFIG_DIR", config_dir.path());

        let fake_mpv = FakeMpv::start().await;
        let fake_subsonic = FakeSubsonic::start().await;

        let mut config = Config::new();
        config.base_url = fake_subsonic.url();
        config.username = "test".into();
        config.password = "test".into();

        let state = new_shared_daemon_state(config.clone());

        let mut mpv = MpvController::with_socket_path(fake_mpv.socket_path.clone());
        mpv.connect_to_existing()
            .await
            .expect("connect to fake mpv socket");

        let core = DaemonCore::new_with_mpv(state.clone(), &config, mpv);

        Self {
            core,
            state,
            fake_mpv,
            fake_subsonic,
            config_dir,
        }
    }
}
