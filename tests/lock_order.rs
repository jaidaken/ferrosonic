//! Concurrent IPC fuzz: each task runs one DaemonRequest variant in a tight loop; if any DaemonCore lock pair is taken in opposite orders by two of these commands the workload deadlocks and the 10s timeout fires. See docs/LOCK-ORDER.md.

mod common;

use std::sync::Arc;
use std::time::Duration;

use common::{song, songs, TestDaemon};
use ferrosonic::ipc::client::{DaemonClient, InProcessClient};
use ferrosonic::ipc::protocol::{DaemonRequest, EnqueueMode};
use serial_test::serial;
use tokio::time::timeout;

const DEADLOCK_BUDGET: Duration = Duration::from_secs(10);
const ITERATIONS_PER_TASK: usize = 25;

async fn seed_mocks(td: &TestDaemon) {
    td.fake_subsonic.expect_ping().await;
    td.fake_subsonic.expect_artists(&[]).await;
    td.fake_subsonic.expect_starred().await;
    td.fake_subsonic.expect_playlists().await;
    td.fake_subsonic.expect_random_songs(&[]).await;
    for i in 0..16 {
        td.fake_subsonic
            .expect_stream_for(&format!("t-{}", i), vec![0u8; 2048])
            .await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[serial]
async fn concurrent_ipc_commands_do_not_deadlock() {
    let td = TestDaemon::new().await;
    seed_mocks(&td).await;
    {
        let mut s = td.state.write().await;
        s.queue = songs("t", 16);
        s.queue_position = Some(0);
    }
    let client = Arc::new(InProcessClient::new(td.core.clone())) as Arc<dyn DaemonClient>;
    let base_url = td.fake_subsonic.url();

    let mut handles = Vec::new();

    let c = client.clone();
    handles.push(tokio::spawn(async move {
        for i in 0..ITERATIONS_PER_TASK {
            let payload: Vec<_> = (0..4)
                .map(|j| song(&format!("t-{}", (i + j) % 16), &format!("Replace {} {}", i, j)))
                .collect();
            let _ = c
                .request(DaemonRequest::EnqueueSongs {
                    songs: payload,
                    mode: EnqueueMode::Replace { play_from: Some(0) },
                })
                .await;
        }
    }));

    let c = client.clone();
    handles.push(tokio::spawn(async move {
        for _ in 0..ITERATIONS_PER_TASK {
            let _ = c.request(DaemonRequest::Next).await;
        }
    }));

    let c = client.clone();
    handles.push(tokio::spawn(async move {
        for _ in 0..ITERATIONS_PER_TASK {
            let _ = c.request(DaemonRequest::Pause).await;
            let _ = c.request(DaemonRequest::Resume).await;
        }
    }));

    let c = client.clone();
    handles.push(tokio::spawn(async move {
        for i in 0..ITERATIONS_PER_TASK {
            let _ = c.request(DaemonRequest::RemoveFromQueue(i % 4)).await;
        }
    }));

    let c = client.clone();
    let url = base_url.clone();
    handles.push(tokio::spawn(async move {
        for i in 0..ITERATIONS_PER_TASK {
            let _ = c
                .request(DaemonRequest::UpdateServerConfig {
                    base_url: url.clone(),
                    username: format!("user-{}", i),
                    password: format!("pw-{}", i).into(),
                })
                .await;
        }
    }));

    let all = async {
        for h in handles {
            let _ = h.await;
        }
    };

    timeout(DEADLOCK_BUDGET, all)
        .await
        .expect("DEADLOCK: IPC command workload exceeded 10s budget");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[serial]
async fn replace_and_remove_interleave_holds_no_locks() {
    let td = TestDaemon::new().await;
    seed_mocks(&td).await;
    {
        let mut s = td.state.write().await;
        s.queue = songs("t", 16);
        s.queue_position = Some(4);
    }
    let client = Arc::new(InProcessClient::new(td.core.clone())) as Arc<dyn DaemonClient>;

    let c1 = client.clone();
    let producer = tokio::spawn(async move {
        for i in 0..50 {
            let payload: Vec<_> = (0..3)
                .map(|j| song(&format!("t-{}", (i + j) % 16), "x"))
                .collect();
            let _ = c1
                .request(DaemonRequest::EnqueueSongs {
                    songs: payload,
                    mode: EnqueueMode::Replace { play_from: Some(0) },
                })
                .await;
        }
    });

    let c2 = client.clone();
    let pruner = tokio::spawn(async move {
        for _ in 0..50 {
            let _ = c2.request(DaemonRequest::RemoveFromQueue(0)).await;
        }
    });

    let work = async {
        let _ = producer.await;
        let _ = pruner.await;
    };

    timeout(DEADLOCK_BUDGET, work)
        .await
        .expect("DEADLOCK: replace/remove interleave exceeded 10s budget");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[serial]
async fn pause_resume_under_replace_storm_stays_consistent() {
    use ferrosonic::app::state::PlaybackState;
    let td = TestDaemon::new().await;
    seed_mocks(&td).await;
    {
        let mut s = td.state.write().await;
        s.queue = songs("t", 16);
        s.queue_position = Some(0);
        s.now_playing.state = PlaybackState::Playing;
    }
    let client = Arc::new(InProcessClient::new(td.core.clone())) as Arc<dyn DaemonClient>;

    let c1 = client.clone();
    let replacer = tokio::spawn(async move {
        for i in 0..30 {
            let payload: Vec<_> = (0..2)
                .map(|j| song(&format!("t-{}", (i + j) % 16), "x"))
                .collect();
            let _ = c1
                .request(DaemonRequest::EnqueueSongs {
                    songs: payload,
                    mode: EnqueueMode::Replace { play_from: Some(0) },
                })
                .await;
        }
    });

    let c2 = client.clone();
    let toggler = tokio::spawn(async move {
        for _ in 0..60 {
            let _ = c2.request(DaemonRequest::TogglePause).await;
        }
    });

    let work = async {
        let _ = replacer.await;
        let _ = toggler.await;
    };

    timeout(DEADLOCK_BUDGET, work)
        .await
        .expect("DEADLOCK: pause/replace storm exceeded 10s budget");

    let s = td.state.read().await;
    assert!(matches!(
        s.now_playing.state,
        PlaybackState::Playing | PlaybackState::Paused | PlaybackState::Stopped
    ));
}
