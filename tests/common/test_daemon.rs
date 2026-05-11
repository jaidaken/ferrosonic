//! Builds a real `DaemonCore` wired to fake mpv + fake Subsonic.
//!
//! Every integration test should construct a `TestDaemon` and drive
//! it through the real public daemon API. Drop cleans up the tempdir
//! and the fake mpv listener; cleanup is best-effort.
//!
//! IMPORTANT: tests using `TestDaemon::new` mutate the process-global
//! `FERROSONIC_CONFIG_DIR` env var. Annotate them with
//! `#[serial_test::serial]` so cargo test runs them one at a time
//! within a binary. Nextest already runs each test in its own
//! process, so the annotation is a no-op there.

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
    /// Spin up fakes, point `FERROSONIC_CONFIG_DIR` at a fresh tempdir,
    /// and connect a real `DaemonCore` to both. Subsonic endpoints
    /// have no mocks yet; tests should add them before exercising any
    /// daemon path that hits the network.
    pub async fn new() -> Self {
        let config_dir = tempfile::tempdir().expect("create config tempdir");
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
