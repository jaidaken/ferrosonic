//! move_queue_item resyncs the gapless preload only when the moved range touches
//! the current track or the slot right after it. Kills the next_maybe_changed
//! comparison mutants (`!(from.max(to) < cur || from.min(to) > cur+1)`): a resync
//! issues an mpv preload (loadfile), so a wrong decision is observable.

mod common;

use common::{songs, TestDaemon};
use ferrosonic::daemon::state::PlaybackState;
use serial_test::serial;

async fn playing_td(n: usize, pos: usize) -> TestDaemon {
    let td = TestDaemon::new().await;
    {
        let mut s = td.state.write().await;
        s.queue = songs("q", n);
        s.queue_position = Some(pos);
        s.now_playing.state = PlaybackState::Playing;
    }
    td
}

fn loadfiles_since(cmds: &[Vec<serde_json::Value>], from: usize) -> usize {
    cmds.iter()
        .skip(from)
        .filter(|c| c.first().and_then(|v| v.as_str()) == Some("loadfile"))
        .count()
}

#[tokio::test]
#[serial]
async fn move_touching_the_current_slot_resyncs_the_preload() {
    let td = playing_td(5, 2).await;
    let before = td.fake_mpv.commands().await.len();
    // cur=2, cur+1=3: moving into index 3 touches the preload slot.
    td.core.move_queue_item(1, 3).await;
    let cmds = td.fake_mpv.commands().await;
    assert!(
        loadfiles_since(&cmds, before) > 0,
        "a move touching the current track or its successor resyncs the gapless preload"
    );
}

#[tokio::test]
#[serial]
async fn move_far_after_the_current_slot_does_not_resync() {
    let td = playing_td(8, 2).await;
    let before = td.fake_mpv.commands().await.len();
    // cur=2, cur+1=3: indices 5 and 6 are entirely past the preload slot.
    td.core.move_queue_item(5, 6).await;
    let cmds = td.fake_mpv.commands().await;
    assert_eq!(
        loadfiles_since(&cmds, before),
        0,
        "a move entirely past the preload slot must not resync"
    );
}
