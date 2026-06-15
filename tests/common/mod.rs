//! Shared test harness. Imported via `mod common;` in each test.

#![allow(dead_code, unused_imports)]

pub mod fake_mpv;
pub mod fake_subsonic;
pub mod fixtures;
pub mod pw_recorder;
pub mod recording_client;
pub mod render;
pub mod test_daemon;

pub use fake_mpv::FakeMpv;
pub use fake_subsonic::FakeSubsonic;
pub use fixtures::{song, song_starred, songs};
pub use pw_recorder::RecordingPwRunner;
pub use recording_client::RecordingClient;
pub use render::{render, render_styled, StyledScreen};
pub use test_daemon::TestDaemon;

/// Throwaway temp dir under a fixed root (`$TMPDIR/ferrosonic-test/`). A
/// SIGKILLed test (nextest timeout, cargo-mutants group-kill) leaks its dir
/// there instead of scattering under `/tmp/.tmpXXXX`; the first call per process
/// reclaims any leak older than an hour, so runs never accumulate.
pub fn tempdir() -> tempfile::TempDir {
    static SWEEP: std::sync::Once = std::sync::Once::new();
    let root = std::env::temp_dir().join("ferrosonic-test");
    let _ = std::fs::create_dir_all(&root);
    SWEEP.call_once(|| sweep_stale_test_dirs(&root));
    tempfile::Builder::new()
        .prefix("ferrosonic-")
        .tempdir_in(&root)
        .expect("create test tempdir under ferrosonic-test root")
}

/// Remove leak dirs older than an hour. Age bound keeps a live test's dir
/// (seconds old) safe; concurrent sweepers race harmlessly via ignored errors.
fn sweep_stale_test_dirs(root: &std::path::Path) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    let now = std::time::SystemTime::now();
    for entry in entries.flatten() {
        let stale = entry
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| now.duration_since(t).ok())
            .is_some_and(|age| age > std::time::Duration::from_secs(3600));
        if stale {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
}
