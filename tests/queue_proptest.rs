//! Property tests for queue mutation invariants.

mod common;

use common::{song, TestDaemon};
use proptest::prelude::*;
use serial_test::serial;

#[derive(Debug, Clone)]
enum Op {
    Push(String),
    Remove(usize),
    Move(usize, usize),
    SetPos(Option<usize>),
}

fn op_strategy(max_id: u8) -> impl Strategy<Value = Op> {
    prop_oneof![
        (0..max_id).prop_map(|id| Op::Push(format!("song-{}", id))),
        (0usize..16).prop_map(Op::Remove),
        (0usize..16, 0usize..16).prop_map(|(f, t)| Op::Move(f, t)),
        prop::option::of(0usize..16).prop_map(Op::SetPos),
    ]
}

fn check_invariants(queue: &[ferrosonic::subsonic::models::Child], pos: Option<usize>) {
    if let Some(p) = pos {
        assert!(
            p < queue.len(),
            "queue_position {} out of bounds (len {})",
            p,
            queue.len()
        );
    }
    let mut seen = std::collections::HashSet::new();
    for s in queue {
        assert!(
            seen.insert(s.id.clone()),
            "duplicate song id {} in queue",
            s.id
        );
    }
}

async fn run_sequence(td: &TestDaemon, ops: Vec<Op>) {
    for op in ops {
        match op {
            Op::Push(id) => {
                let mut s = td.state.write().await;
                if !s.queue.iter().any(|c| c.id == id) {
                    s.queue.push(song(&id, &id));
                }
            }
            Op::Remove(idx) => {
                let len = td.state.read().await.queue.len();
                if idx < len {
                    let client = ferrosonic::ipc::client::InProcessClient::new(td.core.clone());
                    use ferrosonic::ipc::client::DaemonClient;
                    let _ = client
                        .request(ferrosonic::ipc::protocol::DaemonRequest::RemoveFromQueue(
                            idx,
                        ))
                        .await;
                }
            }
            Op::Move(from, to) => td.core.move_queue_item(from, to).await,
            Op::SetPos(p) => {
                let mut s = td.state.write().await;
                if let Some(idx) = p {
                    if idx < s.queue.len() {
                        s.queue_position = Some(idx);
                    }
                } else {
                    s.queue_position = None;
                }
            }
        }
        let s = td.state.read().await;
        check_invariants(&s.queue, s.queue_position);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[serial]
async fn random_op_sequences_preserve_invariants() {
    let mut runner = proptest::test_runner::TestRunner::default();
    runner
        .run(&prop::collection::vec(op_strategy(8), 0..30), |ops| {
            let ops = ops.clone();
            tokio::task::block_in_place(|| {
                tokio::runtime::Handle::current().block_on(async {
                    let td = TestDaemon::new().await;
                    run_sequence(&td, ops).await;
                });
            });
            Ok(())
        })
        .expect("proptest should not find a counterexample");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[serial]
async fn move_queue_item_preserves_current_song_identity() {
    let mut runner = proptest::test_runner::TestRunner::default();
    runner
        .run(
            &(
                1usize..8,
                0usize..8,
                0usize..8,
                0usize..8,
            ),
            |(len, pos, from, to)| {
                let len = len.max(1);
                let pos = pos % len;
                let from = from % len;
                let to = to % len;
                tokio::task::block_in_place(|| {
                    tokio::runtime::Handle::current().block_on(async {
                        let td = TestDaemon::new().await;
                        {
                            let mut s = td.state.write().await;
                            s.queue = (0..len).map(|i| song(&format!("id-{}", i), &format!("t-{}", i))).collect();
                            s.queue_position = Some(pos);
                        }
                        let before_id = {
                            let s = td.state.read().await;
                            s.queue[pos].id.clone()
                        };
                        td.core.move_queue_item(from, to).await;
                        let after = {
                            let s = td.state.read().await;
                            (s.queue_position, s.queue.clone())
                        };
                        let (new_pos_opt, new_queue) = after;
                        let new_pos = new_pos_opt.expect("queue_position must remain Some after move");
                        assert!(
                            new_pos < new_queue.len(),
                            "queue_position {} out of bounds (len {}) after move {}->{}",
                            new_pos,
                            new_queue.len(),
                            from,
                            to
                        );
                        assert_eq!(
                            new_queue[new_pos].id, before_id,
                            "move {}->{} with pos {} broke identity: was {}, now {}",
                            from, to, pos, before_id, new_queue[new_pos].id
                        );
                    });
                });
                Ok(())
            },
        )
        .expect("move_queue_item must preserve the currently-playing song's identity");
}
