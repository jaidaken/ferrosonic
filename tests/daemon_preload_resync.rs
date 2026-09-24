//! move_queue_item resyncs the gapless preload only when the move changes the
//! song that follows the current track. A resync issues an mpv preload
//! (loadfile), so a wrong decision is observable.

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
    // A Playing track means mpv has it loaded: seed the current entry so the
    // preload count gate sees count==1 (the production precondition).
    td.fake_mpv.set_playlist(vec!["current".into()]).await;
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
async fn move_that_changes_the_next_song_resyncs_the_preload() {
    let td = playing_td(5, 2).await;
    let before = td.fake_mpv.commands().await.len();
    // [q0 q1 q2* q3 q4] -> move 3 to 1 -> [q0 q3 q1 q2* q4]: next q3 -> q4.
    td.core.move_queue_item(3, 1).await;
    let cmds = td.fake_mpv.commands().await;
    assert!(
        loadfiles_since(&cmds, before) > 0,
        "the song after the current track changed, so the preload is redone"
    );
}

#[tokio::test]
#[serial]
async fn move_that_keeps_the_next_song_does_not_resync() {
    let td = playing_td(5, 2).await;
    let before = td.fake_mpv.commands().await.len();
    // [q0 q1 q2* q3 q4] -> move 1 to 3 -> [q0 q2* q3 q1 q4]: next stays q3.
    td.core.move_queue_item(1, 3).await;
    let cmds = td.fake_mpv.commands().await;
    assert_eq!(
        loadfiles_since(&cmds, before),
        0,
        "the next song is still q3, so the preload stays"
    );
}

#[tokio::test]
#[serial]
async fn move_landing_exactly_on_the_current_slot_resyncs() {
    let td = playing_td(5, 2).await;
    let before = td.fake_mpv.commands().await.len();
    // from.max(to) == cur (move 0 -> 2, cur=2): the `< cur` boundary. `<`->`==`
    // or `<=` would flip the resync decision here.
    td.core.move_queue_item(0, 2).await;
    let cmds = td.fake_mpv.commands().await;
    assert!(
        loadfiles_since(&cmds, before) > 0,
        "a move landing on the current track resyncs the preload"
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
