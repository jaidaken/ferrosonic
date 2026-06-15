//! Queue snapshot save / load via `QueueSnapshot` and the path env var.

mod common;

use common::{song, songs};
use ferrosonic::daemon::persistence::QueueSnapshot;
use serial_test::serial;

#[tokio::test]
#[serial]
async fn save_then_load_restores_queue_and_position() {
    let dir = common::tempdir();
    std::env::set_var("FERROSONIC_CONFIG_DIR", dir.path());

    let snap = QueueSnapshot {
        queue: songs("t", 4),
        position: Some(2),
    };
    let written = snap.save().expect("save snapshot");
    assert!(written.exists(), "queue.json must exist after save");

    let loaded = QueueSnapshot::load().expect("load returns Some");
    assert_eq!(loaded.queue.len(), 4);
    assert_eq!(loaded.queue[2].id, "t-2");
    assert_eq!(loaded.position, Some(2));
}

#[tokio::test]
#[serial]
async fn load_with_no_file_returns_none() {
    let dir = common::tempdir();
    std::env::set_var("FERROSONIC_CONFIG_DIR", dir.path());

    let r = QueueSnapshot::load();
    assert!(r.is_none(), "missing queue.json should return None");
}

#[tokio::test]
#[serial]
async fn load_with_corrupt_json_returns_none() {
    let dir = common::tempdir();
    std::env::set_var("FERROSONIC_CONFIG_DIR", dir.path());
    std::fs::write(dir.path().join("queue.json"), b"{not valid json").unwrap();

    let r = QueueSnapshot::load();
    assert!(r.is_none(), "corrupt JSON must not crash the loader");
}

#[tokio::test]
#[serial]
async fn save_is_atomic_via_temp_file_rename() {
    let dir = common::tempdir();
    std::env::set_var("FERROSONIC_CONFIG_DIR", dir.path());

    let snap = QueueSnapshot {
        queue: vec![song("a", "A")],
        position: None,
    };
    snap.save().unwrap();

    let entries: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name())
        .collect();
    let names: Vec<String> = entries
        .iter()
        .filter_map(|n| n.to_str().map(String::from))
        .collect();
    assert!(names.contains(&"queue.json".into()));
    assert!(
        !names.iter().any(|n| n.ends_with(".tmp")),
        "no temp file should linger after a successful save"
    );
}

#[tokio::test]
#[serial]
async fn empty_queue_round_trips() {
    let dir = common::tempdir();
    std::env::set_var("FERROSONIC_CONFIG_DIR", dir.path());

    let snap = QueueSnapshot {
        queue: vec![],
        position: None,
    };
    snap.save().unwrap();
    let loaded = QueueSnapshot::load().unwrap();
    assert!(loaded.queue.is_empty());
    assert_eq!(loaded.position, None);
}
