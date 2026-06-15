//! Daemon construction sweeps stale prebuffer temp files left by a crashed
//! prior instance. Kills the `sweep_orphan_prebuffer_files -> ()` mutant, which
//! survives when no test observes that constructing a daemon clears them.

mod common;

use std::time::{Duration, SystemTime};

use common::TestDaemon;
use serial_test::serial;

#[tokio::test]
#[serial]
async fn constructing_a_daemon_sweeps_a_stale_prebuffer_temp_file() {
    let unique = format!("{}-{}", std::process::id(), uniq_suffix());
    let path = std::env::temp_dir().join(format!("ferrosonic-prebuf-{unique}.dat"));
    std::fs::write(&path, b"orphan").expect("write orphan temp file");
    // Backdate past the 300s age gate so the sweep treats it as crash debris.
    let stale = SystemTime::now() - Duration::from_secs(600);
    std::fs::File::open(&path)
        .and_then(|f| f.set_modified(stale))
        .expect("backdate orphan mtime");
    assert!(
        path.exists(),
        "precondition: the orphan file exists before construction"
    );

    let _td = TestDaemon::new().await;

    assert!(
        !path.exists(),
        "daemon construction must sweep the stale prebuffer temp file"
    );
}

fn uniq_suffix() -> u128 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0)
}
