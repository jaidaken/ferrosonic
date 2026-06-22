//! mpv end-file event listener gating (`core::spawn_mpv_event_listener`).
//!
//! Regression cover for the documented mutation seam: the `reason != "eof"`
//! skip and the `count >= 2` gapless-preload gate. Uses the `FakeMpv`
//! unsolicited-event injection seam to push real `end-file` messages.

mod common;

use common::{songs, TestDaemon};
use ferrosonic::daemon::state::PlaybackState;
use serde_json::Value;
use serial_test::serial;

fn advanced_to(cmds: &[Vec<Value>], id: &str) -> bool {
    cmds.iter().any(|c| {
        c.first().and_then(Value::as_str) == Some("loadfile")
            && c.get(1)
                .and_then(Value::as_str)
                .is_some_and(|p| p.contains(id))
    })
}

fn queried_playlist_count(cmds: &[Vec<Value>]) -> bool {
    cmds.iter().any(|c| {
        c.first().and_then(Value::as_str) == Some("get_property")
            && c.get(1).and_then(Value::as_str) == Some("playlist-count")
    })
}

async fn playing_3track(td: &TestDaemon) {
    td.fake_subsonic.expect_ping().await;
    let mut s = td.state.write().await;
    s.queue = songs("t", 3);
    s.queue_position = Some(0);
    s.now_playing.song = Some(s.queue[0].clone());
    s.now_playing.state = PlaybackState::Playing;
    s.now_playing.duration = 100.0;
}

#[tokio::test]
#[serial]
async fn eof_with_no_preload_advances_to_next_track() {
    let td = TestDaemon::new().await;
    playing_3track(&td).await;
    td.fake_mpv.set_loaded_file("local.mp3").await;
    // playlist-count 1 (< 2): no gapless preload, the listener owns the advance.
    td.fake_mpv.set_playlist(vec!["local.mp3".into()]).await;

    let _listener = td.core.spawn_mpv_event_listener().await;
    td.fake_mpv.emit_end_file("eof").await;

    assert!(
        td.fake_mpv
            .wait_for(5000, |c| advanced_to(c, "id=t-1"))
            .await,
        "eof with playlist-count < 2 must advance to the next track"
    );
}

#[tokio::test]
#[serial]
async fn eof_during_gapless_preload_does_not_advance() {
    let td = TestDaemon::new().await;
    playing_3track(&td).await;
    td.fake_mpv.set_loaded_file("local.mp3").await;
    // playlist-count 2 (>= 2): a gapless next track is preloaded, so the
    // polling path owns the advance and the event listener must defer.
    td.fake_mpv
        .set_playlist(vec!["local.mp3".into(), "local2.mp3".into()])
        .await;

    let _listener = td.core.spawn_mpv_event_listener().await;
    td.fake_mpv.emit_end_file("eof").await;

    assert!(
        td.fake_mpv.wait_for(5000, queried_playlist_count).await,
        "listener should query playlist-count when handling an eof event"
    );
    assert!(
        !td.fake_mpv
            .wait_for(300, |c| advanced_to(c, "id=t-1"))
            .await,
        "eof with playlist-count >= 2 must not advance; the poll owns it"
    );
}

#[tokio::test]
#[serial]
async fn non_eof_endfile_is_ignored_but_eof_still_advances() {
    let td = TestDaemon::new().await;
    playing_3track(&td).await;
    td.fake_mpv.set_loaded_file("local.mp3").await;
    td.fake_mpv.set_playlist(vec!["local.mp3".into()]).await;

    let _listener = td.core.spawn_mpv_event_listener().await;

    // A non-eof end-file (e.g. user stop) must not trigger an advance.
    td.fake_mpv.emit_end_file("stop").await;
    assert!(
        !td.fake_mpv
            .wait_for(300, |c| advanced_to(c, "id=t-1"))
            .await,
        "a non-eof end-file must be ignored, not advance"
    );

    // The same listener still advances on a real eof, proving it was live
    // and that the stop above was skipped by the `reason != \"eof\"` gate.
    td.fake_mpv.emit_end_file("eof").await;
    assert!(
        td.fake_mpv
            .wait_for(5000, |c| advanced_to(c, "id=t-1"))
            .await,
        "eof after the ignored stop must advance"
    );
}
