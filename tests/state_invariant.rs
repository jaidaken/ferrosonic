//! STATE_INVARIANT regression tests for the prompt 2.5 checklist; one test per item in docs/STABILIZATION.md section 5.

mod common;

use common::{songs, TestDaemon};
use serial_test::serial;

/// R1 core.rs:261. restore_queue_blocking used try_write and warned on contention; fix lifts the snapshot load into new_shared_daemon_state so it happens before the Arc<RwLock> is shared. Test asserts restoration actually lands; pre-fix this passes because construction is uncontended in tests but the silent-skip path remained reachable.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[serial]
async fn r1_restore_queue_blocking_does_not_silently_skip() {
    let config_dir = tempfile::tempdir().expect("create config tempdir");
    std::env::set_var("FERROSONIC_CONFIG_DIR", config_dir.path());

    let snap = ferrosonic::daemon::persistence::QueueSnapshot {
        queue: songs("t", 5),
        position: Some(2),
    };
    snap.save().expect("save snapshot");

    let td = TestDaemon::new_with_config_dir(config_dir).await;
    let s = td.state.read().await;
    assert_eq!(s.queue.len(), 5, "queue must restore from snapshot");
    assert_eq!(s.queue_position, Some(2), "position must restore");
}
