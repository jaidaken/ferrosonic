//! Offline track cache: streamed tracks populate the cache and are then
//! served from disk; a disabled cache writes nothing.

mod common;

use std::time::Duration;

use common::{song, TestDaemon};
use ferrosonic::daemon::core::PlayMode;
use serde_json::Value;
use serial_test::serial;

fn payload(size: usize) -> Vec<u8> {
    vec![0u8; size]
}

fn cache_files(tracks_dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    std::fs::read_dir(tracks_dir)
        .map(|entries| {
            entries
                .flatten()
                .map(|e| e.path())
                .filter(|p| {
                    p.extension().and_then(|e| e.to_str()) == Some("audio")
                        || p.file_name()
                            .and_then(|n| n.to_str())
                            .is_some_and(|n| n.ends_with(".audio"))
                })
                .collect()
        })
        .unwrap_or_default()
}

async fn wait_for_cache_file(tracks_dir: &std::path::Path) -> Option<std::path::PathBuf> {
    for _ in 0..200 {
        if let Some(path) = cache_files(tracks_dir).into_iter().next() {
            return Some(path);
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    None
}

#[tokio::test]
#[serial]
async fn streamed_track_is_cached_and_then_played_from_disk() {
    let config_dir = common::tempdir();
    let cache_dir = common::tempdir();
    std::env::set_var("FERROSONIC_CONFIG_DIR", config_dir.path());
    std::env::set_var("FERROSONIC_CACHE_DIR", cache_dir.path());
    let tracks_dir = cache_dir.path().join("tracks");

    let td = TestDaemon::new().await;
    td.fake_subsonic
        .expect_stream_for("abc", payload(64 * 1024))
        .await;
    td.core.set_offline_cache_enabled(true).await.unwrap();
    {
        let mut s = td.state.write().await;
        s.queue.push(song("abc", "Track"));
    }

    td.core
        .play_queue_position(0, PlayMode::Direct)
        .await
        .unwrap();

    let cached = wait_for_cache_file(&tracks_dir).await;
    let cached = cached.expect("streamed track must be cached");
    assert!(cached.exists());
    // Give the index write that follows the rename a moment to land.
    tokio::time::sleep(Duration::from_millis(50)).await;

    // A second play must load the local cached copy, not re-fetch.
    let before = td.fake_mpv.commands().await.len();
    td.core
        .play_queue_position(0, PlayMode::Direct)
        .await
        .unwrap();
    let commands = td.fake_mpv.commands().await;
    let loadfiles: Vec<String> = commands[before..]
        .iter()
        .filter(|c| c.first().and_then(Value::as_str) == Some("loadfile"))
        .filter_map(|c| c.get(1).and_then(Value::as_str).map(String::from))
        .collect();
    assert!(
        loadfiles
            .iter()
            .any(|path| path == &cached.to_string_lossy()),
        "second play must load the cached path {cached:?}; got {loadfiles:?}"
    );
}

#[tokio::test]
#[serial]
async fn disabled_cache_writes_nothing() {
    let config_dir = common::tempdir();
    let cache_dir = common::tempdir();
    std::env::set_var("FERROSONIC_CONFIG_DIR", config_dir.path());
    std::env::set_var("FERROSONIC_CACHE_DIR", cache_dir.path());
    let tracks_dir = cache_dir.path().join("tracks");

    let td = TestDaemon::new().await;
    td.fake_subsonic
        .expect_stream_for("abc", payload(16 * 1024))
        .await;
    {
        let mut s = td.state.write().await;
        s.queue.push(song("abc", "Track"));
    }

    td.core
        .play_queue_position(0, PlayMode::Direct)
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;

    assert!(
        cache_files(&tracks_dir).is_empty(),
        "a disabled cache must not write files"
    );
}
