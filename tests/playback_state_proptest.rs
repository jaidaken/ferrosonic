//! Property test: arbitrary playback-control sequences keep daemon state
//! consistent and never panic. Guards the pause/resume/seek/skip state machine
//! where the resume-restart bug lived.
//!
//! After every op: queue_position (if set) is in bounds, the playhead is finite
//! and non-negative, and while Playing/Paused the now-playing song matches the
//! song at the queue position. Slow + randomized: excluded from mutation runs.

mod common;

use common::{songs, TestDaemon};
use ferrosonic::daemon::core::PlayMode;
use ferrosonic::daemon::state::PlaybackState;
use proptest::prelude::*;
use serial_test::serial;

#[derive(Debug, Clone)]
enum Op {
    Play(usize),
    Pause,
    Resume,
    Toggle,
    Next,
    Prev,
    Seek(f64),
    Stop,
    StopKeep,
}

fn op_strategy() -> impl Strategy<Value = Op> {
    prop_oneof![
        (0usize..12).prop_map(Op::Play),
        Just(Op::Pause),
        Just(Op::Resume),
        Just(Op::Toggle),
        Just(Op::Next),
        Just(Op::Prev),
        (0.0f64..300.0).prop_map(Op::Seek),
        Just(Op::Stop),
        Just(Op::StopKeep),
    ]
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[serial]
async fn arbitrary_control_sequences_keep_state_consistent() {
    let mut runner = proptest::test_runner::TestRunner::default();
    runner
        .run(&prop::collection::vec(op_strategy(), 0..25), |ops| {
            tokio::task::block_in_place(|| {
                tokio::runtime::Handle::current().block_on(async {
                    let td = TestDaemon::new().await;
                    {
                        let mut s = td.state.write().await;
                        s.queue = songs("t", 10);
                    }
                    for op in &ops {
                        match op {
                            Op::Play(p) => {
                                let _ = td.core.play_queue_position(*p, PlayMode::Direct).await;
                            }
                            Op::Pause => {
                                let _ = td.core.pause_playback().await;
                            }
                            Op::Resume => {
                                let _ = td.core.resume_playback().await;
                            }
                            Op::Toggle => {
                                let _ = td.core.toggle_pause().await;
                            }
                            Op::Next => {
                                let _ = td.core.next_track().await;
                            }
                            Op::Prev => {
                                let _ = td.core.prev_track().await;
                            }
                            Op::Seek(s) => {
                                let _ = td.core.seek(*s).await;
                            }
                            Op::Stop => {
                                let _ = td.core.stop_playback().await;
                            }
                            Op::StopKeep => {
                                let _ = td.core.stop_keep_queue().await;
                            }
                        }

                        let s = td.state.read().await;
                        let qlen = s.queue.len();
                        if let Some(pos) = s.queue_position {
                            prop_assert!(
                                qlen == 0 || pos < qlen,
                                "queue_position {pos} out of bounds (len {qlen}) after {op:?}"
                            );
                        }
                        let position = s.now_playing.position;
                        prop_assert!(
                            position.is_finite() && position >= 0.0,
                            "playhead invalid ({position}) after {op:?}"
                        );
                        if matches!(
                            s.now_playing.state,
                            PlaybackState::Playing | PlaybackState::Paused
                        ) {
                            if let (Some(pos), Some(song)) =
                                (s.queue_position, s.now_playing.song.as_ref())
                            {
                                if pos < qlen {
                                    prop_assert_eq!(
                                        &song.id,
                                        &s.queue[pos].id,
                                        "now-playing song mismatches queue position after {:?}",
                                        op
                                    );
                                }
                            }
                        }
                    }
                    Ok(())
                })
            })
        })
        .unwrap();
}
