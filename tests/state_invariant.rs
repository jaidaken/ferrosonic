//! STATE_INVARIANT regression tests for the prompt 2.5 checklist; one test per item in docs/STABILIZATION.md section 5.

mod common;

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

use common::{song, songs, TestDaemon};
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

/// R4 core.rs:1902. update_server_config must publish the new subsonic client and the bumped config_gen atomically so a concurrent refresh cannot read (new client, old gen) and commit stale results.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[serial]
async fn r4_update_server_config_bumps_gen_before_installing_client() {
    let td = TestDaemon::new().await;
    td.fake_subsonic.expect_ping().await;
    td.fake_subsonic.expect_artists(&[]).await;
    td.fake_subsonic.expect_starred().await;
    td.fake_subsonic.expect_playlists().await;
    td.fake_subsonic.expect_random_songs(&[]).await;

    let alt = common::FakeSubsonic::start().await;
    alt.expect_ping().await;
    alt.expect_artists(&[]).await;
    alt.expect_starred().await;
    alt.expect_playlists().await;
    alt.expect_random_songs(&[]).await;
    let alt_url = alt.url();

    let observed = Arc::new(tokio::sync::Mutex::new(None::<(u64, String)>));
    let stop = Arc::new(AtomicBool::new(false));

    let core = td.core.clone();
    let alt_url_probe = alt_url.clone();
    let stop_clone = stop.clone();
    let observed_clone = observed.clone();
    let racer = tokio::spawn(async move {
        while !stop_clone.load(Ordering::Acquire) {
            let snapshot = {
                let guard = core.subsonic.read().await;
                let gen = core.config_gen_for_test();
                guard.as_ref().map(|c| (gen, c.base_url().to_string()))
            };
            if let Some((gen, url)) = snapshot {
                if gen == 0 && url.trim_end_matches('/') == alt_url_probe.trim_end_matches('/') {
                    *observed_clone.lock().await = Some((gen, url));
                }
            }
            tokio::task::yield_now().await;
        }
    });

    let _ = td
        .core
        .update_server_config(&alt_url, "user", "pw")
        .await;

    stop.store(true, Ordering::Release);
    let _ = racer.await;

    assert!(
        td.core.config_gen_for_test() >= 1,
        "config_gen must bump on update_server_config"
    );
    let leaked = observed.lock().await.clone();
    assert!(
        leaked.is_none(),
        "observed new client at config_gen=0 (saw {:?}); bump must precede install under one critical section",
        leaked
    );
}

/// R2 core.rs:588. apply_star_to_cached + refresh_starred must publish atomically: observers must never see starred_ids contain song_id while starred_songs lacks the corresponding row.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[serial]
async fn r2_apply_star_and_refresh_under_one_lock() {
    let td = TestDaemon::new().await;
    td.fake_subsonic.expect_ping().await;
    td.fake_subsonic.expect_artists(&[]).await;
    td.fake_subsonic.expect_starred_with(&["Track 0"]).await;
    td.fake_subsonic.expect_playlists().await;
    td.fake_subsonic.expect_random_songs(&[]).await;
    td.fake_subsonic.expect_star().await;
    td.fake_subsonic.expect_unstar().await;
    {
        let mut s = td.state.write().await;
        s.queue = vec![song("starred-0", "Track 0")];
        s.queue_position = Some(0);
    }

    let state_handle = td.state.clone();
    let stop = Arc::new(AtomicBool::new(false));
    let stop_clone = stop.clone();
    let violations = Arc::new(AtomicUsize::new(0));
    let v = violations.clone();
    let checker = tokio::spawn(async move {
        while !stop_clone.load(Ordering::Acquire) {
            let s = state_handle.read().await;
            let in_ids = s.library.starred_ids.contains("starred-0");
            let in_vec = s
                .library
                .starred_songs
                .iter()
                .any(|c| c.id == "starred-0");
            if in_ids != in_vec {
                v.fetch_add(1, Ordering::Relaxed);
            }
            drop(s);
            tokio::task::yield_now().await;
        }
    });

    for _ in 0..4 {
        let _ = td.core.toggle_star_song("starred-0").await;
    }
    stop.store(true, Ordering::Release);
    let _ = checker.await;

    assert_eq!(
        violations.load(Ordering::Acquire),
        0,
        "observers saw starred_ids/starred_songs desync between apply_star_to_cached and refresh_starred"
    );
}
