//! App::run setup-phase test seams.

mod common;
use ferrosonic::app::App;
use ferrosonic::config::Config;
use serial_test::serial;

struct AppFixture {
    app: App,
    _tempdir: tempfile::TempDir,
}

async fn build_app() -> AppFixture {
    let tempdir = common::tempdir();
    std::env::set_var("FERROSONIC_CONFIG_DIR", tempdir.path());
    let mut config = Config::new();
    config.daemon = false;
    let app = App::new(config);
    AppFixture {
        app,
        _tempdir: tempdir,
    }
}

#[tokio::test]
#[serial]
async fn load_and_apply_themes_populates_settings_themes() {
    let fx = build_app().await;
    fx.app.load_and_apply_themes().await;
    let cs = fx.app.client_state.read().await;
    assert!(
        !cs.settings_state.themes.is_empty(),
        "themes must be loaded"
    );
}

#[tokio::test]
#[serial]
async fn load_and_apply_themes_selects_default_when_config_theme_is_empty() {
    let fx = build_app().await;
    fx.app.load_and_apply_themes().await;
    let cs = fx.app.client_state.read().await;
    let theme = cs.settings_state.current_theme();
    assert!(!theme.name.is_empty());
}

#[tokio::test]
#[serial]
async fn load_and_apply_themes_uses_configured_theme_name() {
    let fx = build_app().await;
    {
        let mut ds = fx.app.daemon_state.write().await;
        ds.config.theme = "default".into();
    }
    fx.app.load_and_apply_themes().await;
    let cs = fx.app.client_state.read().await;
    let theme = cs.settings_state.current_theme();
    assert!(!theme.name.is_empty());
}

#[tokio::test]
#[serial]
async fn probe_cava_available_updates_client_state() {
    let fx = build_app().await;
    fx.app.probe_cava_available().await;
    let cs = fx.app.client_state.read().await;
    let expected = std::process::Command::new("which")
        .arg("cava")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    assert_eq!(cs.cava_available, expected);
}

#[tokio::test]
#[serial]
async fn probe_cava_unavailable_disables_cava_setting() {
    let fx = build_app().await;
    {
        let mut cs = fx.app.client_state.write().await;
        cs.settings_state.cava_enabled = true;
    }
    let no_path = common::tempdir();
    let saved_path = std::env::var_os("PATH");
    std::env::set_var("PATH", no_path.path());
    fx.app.probe_cava_available().await;
    if let Some(p) = saved_path {
        std::env::set_var("PATH", p);
    } else {
        std::env::remove_var("PATH");
    }
    let cs = fx.app.client_state.read().await;
    if !cs.cava_available {
        assert!(
            !cs.settings_state.cava_enabled,
            "cava_enabled must be cleared when binary missing"
        );
    }
}

#[tokio::test]
#[serial]
async fn start_mpv_with_notification_runs_without_panic() {
    let fx = build_app().await;
    fx.app.start_mpv_with_notification().await;
}

#[tokio::test]
#[serial]
async fn start_mpv_with_no_core_in_remote_mode_is_silent() {
    let tempdir = common::tempdir();
    std::env::set_var("FERROSONIC_CONFIG_DIR", tempdir.path());
    let mut config = Config::new();
    config.daemon = true;
    let rec = std::sync::Arc::new(ferrosonic::ipc::client::InProcessClient::new(
        ferrosonic::daemon::DaemonCore::new(
            ferrosonic::app::state::new_shared_daemon_state(config.clone()),
            &config,
        ),
    ));
    let app = App::with_remote_client(rec, config);
    app.start_mpv_with_notification().await;
}

#[tokio::test]
#[serial]
async fn load_initial_data_sets_starred_option() {
    use ferrosonic::app::models::SongOption;
    let mut fx = build_app().await;
    fx.app.load_initial_data().await;
    let cs = fx.app.client_state.read().await;
    assert_eq!(cs.songs.selected_option, Some(SongOption::Starred));
}
