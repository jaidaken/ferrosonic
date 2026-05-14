//! STATE_INVARIANT regression tests for the prompt 2.5 checklist; one test per item in docs/STABILIZATION.md section 5.

mod common;

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

use common::{song, songs, TestDaemon};
use ferrosonic::app::state::PlaybackState;
use ferrosonic::daemon::core::PlayMode;
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
        .update_server_config(&alt_url, "user", &"pw".into())
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

/// R2 core.rs:984. commit_play_state_in_lock sets state.Playing under the state write lock; the idle-advance observer in update_playback_info must not interpret state.Playing+mpv.idle as track-ended during the loadfile in-flight window. The fix stamps last_loadfile at commit time so the 1.5s acceptance gate covers the entire window.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[serial]
async fn r2_commit_play_state_stamps_last_loadfile_invariant() {
    let td = TestDaemon::new().await;
    td.fake_subsonic.expect_ping().await;
    td.fake_subsonic.expect_artists(&[]).await;
    td.fake_subsonic.expect_starred().await;
    td.fake_subsonic.expect_playlists().await;
    td.fake_subsonic.expect_random_songs(&[]).await;
    for i in 0..4 {
        td.fake_subsonic
            .expect_stream_for(&format!("song-{}", i), vec![0u8; 1024])
            .await;
    }
    {
        let mut s = td.state.write().await;
        s.queue = songs("song", 4);
        s.queue_position = None;
    }

    td.fake_mpv.set_fail_loadfile(true).await;
    let _ = td.core.play_queue_position(0, PlayMode::Direct).await;

    let state_after = td.state.read().await;
    assert_eq!(
        state_after.now_playing.state,
        PlaybackState::Playing,
        "state.Playing is committed before the loadfile attempt"
    );
    let qp_before = state_after.queue_position;
    drop(state_after);

    td.core.update_playback_info().await;

    let state_post = td.state.read().await;
    assert_eq!(
        state_post.queue_position, qp_before,
        "observer must not auto-advance during the commit-to-loadfile window"
    );
}

/// R1+R2 core.rs:639. toggle_pause reads playback state outside the lock then commits Paused/Playing after mpv ack; under a concurrent Replace storm the queue must stay coherent and now_playing.state must remain in {Playing,Paused,Stopped}.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[serial]
async fn r1_toggle_pause_state_stays_consistent_under_replace() {
    use ferrosonic::ipc::client::{DaemonClient, InProcessClient};
    use ferrosonic::ipc::protocol::{DaemonRequest, EnqueueMode};
    use tokio::time::{timeout, Duration};

    let td = TestDaemon::new().await;
    td.fake_subsonic.expect_ping().await;
    td.fake_subsonic.expect_artists(&[]).await;
    td.fake_subsonic.expect_starred().await;
    td.fake_subsonic.expect_playlists().await;
    td.fake_subsonic.expect_random_songs(&[]).await;
    for i in 0..16 {
        td.fake_subsonic
            .expect_stream_for(&format!("song-{}", i), vec![0u8; 1024])
            .await;
    }
    {
        let mut s = td.state.write().await;
        s.queue = songs("song", 16);
        s.queue_position = Some(0);
        s.now_playing.state = PlaybackState::Playing;
        s.now_playing.song = Some(s.queue[0].clone());
    }
    let client = Arc::new(InProcessClient::new(td.core.clone())) as Arc<dyn DaemonClient>;

    let c1 = client.clone();
    let toggler = tokio::spawn(async move {
        for _ in 0..40 {
            let _ = c1.request(DaemonRequest::TogglePause).await;
        }
    });

    let c2 = client.clone();
    let replacer = tokio::spawn(async move {
        for i in 0..20 {
            let payload: Vec<_> = (0..3)
                .map(|j| song(&format!("song-{}", (i + j) % 16), "x"))
                .collect();
            let _ = c2
                .request(DaemonRequest::EnqueueSongs {
                    songs: payload,
                    mode: EnqueueMode::Replace { play_from: Some(0) },
                })
                .await;
        }
    });

    let work = async {
        let _ = toggler.await;
        let _ = replacer.await;
    };
    timeout(Duration::from_secs(10), work)
        .await
        .expect("workload exceeded budget");

    let s = td.state.read().await;
    if let Some(pos) = s.queue_position {
        assert!(
            pos < s.queue.len(),
            "queue_position {} out of bounds for queue len {}",
            pos,
            s.queue.len()
        );
    }
    assert!(matches!(
        s.now_playing.state,
        PlaybackState::Playing | PlaybackState::Paused | PlaybackState::Stopped
    ));
}

/// R1 core.rs:673. pause_playback read state under a read lock, early-returned, then took mpv lock and committed Paused; the gap between the initial check and the final commit allowed a concurrent Stop to be overwritten by Paused.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[serial]
async fn r1_pause_playback_rechecks_under_write_lock() {
    use ferrosonic::ipc::client::{DaemonClient, InProcessClient};
    use ferrosonic::ipc::protocol::DaemonRequest;
    use tokio::time::{timeout, Duration};

    let td = TestDaemon::new().await;
    td.fake_subsonic.expect_ping().await;
    td.fake_subsonic.expect_artists(&[]).await;
    td.fake_subsonic.expect_starred().await;
    td.fake_subsonic.expect_playlists().await;
    td.fake_subsonic.expect_random_songs(&[]).await;
    for i in 0..8 {
        td.fake_subsonic
            .expect_stream_for(&format!("song-{}", i), vec![0u8; 1024])
            .await;
    }
    {
        let mut s = td.state.write().await;
        s.queue = songs("song", 8);
        s.queue_position = Some(0);
        s.now_playing.state = PlaybackState::Playing;
        s.now_playing.song = Some(s.queue[0].clone());
    }
    let client = Arc::new(InProcessClient::new(td.core.clone())) as Arc<dyn DaemonClient>;

    let c1 = client.clone();
    let stopper = tokio::spawn(async move {
        for _ in 0..20 {
            let _ = c1.request(DaemonRequest::Stop).await;
        }
    });

    let c2 = client.clone();
    let pauser = tokio::spawn(async move {
        for _ in 0..40 {
            let _ = c2.request(DaemonRequest::Pause).await;
        }
    });

    let work = async {
        let _ = stopper.await;
        let _ = pauser.await;
    };
    timeout(Duration::from_secs(10), work)
        .await
        .expect("workload exceeded budget");

    let s = td.state.read().await;
    if s.queue.is_empty() {
        assert_eq!(
            s.now_playing.state,
            PlaybackState::Stopped,
            "empty queue implies Stopped"
        );
    }
}

/// R1 core.rs:778. extend_with_random_and_play must read queue.len, extend, and commit play state under a single state write lock so a concurrent advance_auto reading queue.len between extend and commit cannot stamp the wrong queue_position. The function as shipped does the entire read-validate-extend-play under one write; this test pins that contract by verifying that immediately after the call the queue_position points to a song whose id matches an appended random song.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[serial]
async fn r1_extend_with_random_and_play_atomic_queue_extend() {
    let td = TestDaemon::new().await;
    td.fake_subsonic.expect_ping().await;
    td.fake_subsonic.expect_artists(&[]).await;
    td.fake_subsonic.expect_starred().await;
    td.fake_subsonic.expect_playlists().await;
    td.fake_subsonic
        .expect_random_songs(&["A", "B", "C", "D"])
        .await;
    for i in 0..4 {
        td.fake_subsonic
            .expect_stream_for(&format!("song-{}", i), vec![0u8; 1024])
            .await;
    }

    {
        let mut s = td.state.write().await;
        s.queue = songs("seed", 1);
        s.queue_position = Some(0);
        s.config.auto_continue = true;
        s.now_playing.state = PlaybackState::Playing;
        s.now_playing.song = Some(s.queue[0].clone());
    }

    let initial_len = td.state.read().await.queue.len();
    let _ = td.core.next_track().await;

    let s = td.state.read().await;
    assert!(
        s.queue.len() > initial_len,
        "queue must extend with random songs (was {}, now {})",
        initial_len,
        s.queue.len()
    );
    let pos = s.queue_position.expect("queue_position must be set after auto-continue");
    assert_eq!(
        pos, initial_len,
        "queue_position must point at the first appended song"
    );
    let played_song = s.now_playing.song.as_ref().expect("now_playing.song set");
    assert_eq!(
        played_song.id,
        s.queue[pos].id,
        "now_playing.song must equal queue[queue_position] at commit time"
    );
    let random_ids: std::collections::HashSet<&str> =
        ["song-0", "song-1", "song-2", "song-3"].iter().copied().collect();
    assert!(
        random_ids.contains(played_song.id.as_str()),
        "played song {} must come from the random batch",
        played_song.id
    );
}
