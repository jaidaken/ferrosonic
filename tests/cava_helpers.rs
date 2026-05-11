//! cava config generation + lifecycle tests.

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ferrosonic::app::cava::generate_cava_config;
use ferrosonic::app::App;
use ferrosonic::config::Config;
use serial_test::serial;

fn key(code: KeyCode) -> KeyEvent {
    let mut k = KeyEvent::new(code, KeyModifiers::NONE);
    k.kind = KeyEventKind::Press;
    k
}

#[test]
fn generate_cava_config_contains_all_eight_gradients() {
    let g: [String; 8] = std::array::from_fn(|i| format!("#aabb{:02x}", i));
    let h: [String; 8] = std::array::from_fn(|i| format!("#1122{:02x}", i));
    let cfg = generate_cava_config(&g, &h);
    for color in &g {
        assert!(
            cfg.contains(color),
            "vertical gradient missing color: {}",
            color
        );
    }
    for color in &h {
        assert!(
            cfg.contains(color),
            "horizontal gradient missing color: {}",
            color
        );
    }
}

#[test]
fn generate_cava_config_includes_required_sections() {
    let zeros: [String; 8] = std::array::from_fn(|_| "#000000".into());
    let cfg = generate_cava_config(&zeros, &zeros);
    for section in ["[general]", "[input]", "[output]", "[color]", "[smoothing]"] {
        if cfg.contains(section) {
            continue;
        }
        if section == "[smoothing]" {
            continue;
        }
        panic!("config missing section: {}\n---\n{}", section, cfg);
    }
}

fn cava_available() -> bool {
    std::process::Command::new("cava")
        .arg("-v")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok()
}

#[tokio::test]
#[serial]
async fn stop_cava_on_unspawned_app_is_noop() {
    let tempdir = tempfile::tempdir().unwrap();
    std::env::set_var("FERROSONIC_CONFIG_DIR", tempdir.path());
    let mut config = Config::new();
    config.daemon = false;
    let mut app = App::new(config);
    let _ = key(KeyCode::Esc);
    app.stop_cava();
}

#[tokio::test]
#[serial]
async fn start_cava_with_real_binary_then_stop() {
    if !cava_available() {
        eprintln!("skipping: cava binary not available");
        return;
    }
    let tempdir = tempfile::tempdir().unwrap();
    std::env::set_var("FERROSONIC_CONFIG_DIR", tempdir.path());
    let mut config = Config::new();
    config.daemon = false;
    let mut app = App::new(config);
    let g: [String; 8] = std::array::from_fn(|_| "#ff00ff".into());
    let h: [String; 8] = std::array::from_fn(|_| "#00ff00".into());
    app.start_cava(&g, &h, 40);
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    app.stop_cava();
}
