//! Stress proptest: random play_queue_position sequences. Excluded from mutation runs.

mod common;

use common::{songs, TestDaemon};
use ferrosonic::daemon::core::PlayMode;
use proptest::prelude::*;
use serial_test::serial;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[serial]
async fn proptest_arbitrary_play_queue_sequences_dont_panic() {
    let mut runner = proptest::test_runner::TestRunner::default();
    runner
        .run(&prop::collection::vec(0usize..30, 0..40), |positions| {
            tokio::task::block_in_place(|| {
                tokio::runtime::Handle::current().block_on(async {
                    let td = TestDaemon::new().await;
                    {
                        let mut s = td.state.write().await;
                        s.queue = songs("t", 30);
                    }
                    for p in positions {
                        let _ = td.core.play_queue_position(p, PlayMode::Direct).await;
                    }
                });
            });
            Ok(())
        })
        .unwrap();
}
