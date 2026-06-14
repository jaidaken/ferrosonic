//! Regression: pause then resume must reload the track at the saved playhead,
//! not restart at 0.
//!
//! Pause stops mpv (to free the audio device); resume reloads the track. The
//! bug issued a `seek` immediately after `loadfile`, which races mpv's async
//! network load on real mpv, so the seek is lost and the track restarts. The
//! fix loads at the offset (`loadfile ... start=<secs>`) so there is no seek to
//! race. This asserts the daemon's command to mpv, via the fake mpv server.

mod common;

use common::{song, TestDaemon};
use ferrosonic::daemon::core::PlayMode;
use serial_test::serial;

#[tokio::test]
#[serial]
async fn resume_after_pause_reloads_at_saved_offset_not_zero() {
    let td = TestDaemon::new().await;
    {
        let mut s = td.state.write().await;
        s.queue = vec![song("s1", "Track One"), song("s2", "Track Two")];
        s.queue_position = Some(0);
    }
    td.core
        .play_queue_position(0, PlayMode::Direct)
        .await
        .unwrap();

    // Simulate 30s of playback elapsed, then pause (which freezes the playhead).
    {
        let mut s = td.state.write().await;
        s.now_playing.position = 30.0;
    }
    td.core.pause_playback().await.unwrap();

    td.core.resume_playback().await.unwrap();

    // Resume reload is the last `loadfile replace`; it must carry a start offset.
    let cmds = td.fake_mpv.commands().await;
    let resume_reload = cmds
        .iter()
        .rev()
        .find(|c| {
            c.first().and_then(|v| v.as_str()) == Some("loadfile")
                && c.get(2).and_then(|v| v.as_str()) == Some("replace")
        })
        .expect("resume must issue a replace loadfile");
    let opts = resume_reload
        .get(4)
        .and_then(|v| v.as_str())
        .unwrap_or_default();

    assert!(
        opts.contains("start=30"),
        "resume must reload at the saved 30s offset (loadfile start=30); \
         resume loadfile options were {opts:?}; all commands: {cmds:?}"
    );
}

#[tokio::test]
#[serial]
async fn play_at_zero_offset_uses_plain_loadfile_without_start() {
    // dispatch_play loads at the offset only when start_at > 0; the `>`->`>=`
    // boundary mutant would emit "start=0" for a normal play.
    let td = TestDaemon::new().await;
    {
        let mut s = td.state.write().await;
        s.queue = vec![song("s1", "Track One")];
        s.queue_position = Some(0);
    }
    td.core
        .play_queue_position_at(0, PlayMode::Direct, 0.0)
        .await
        .unwrap();

    let cmds = td.fake_mpv.commands().await;
    let replace_loadfile = cmds
        .iter()
        .find(|c| {
            c.first().and_then(|v| v.as_str()) == Some("loadfile")
                && c.get(2).and_then(|v| v.as_str()) == Some("replace")
        })
        .expect("a replace loadfile");
    let opts = replace_loadfile.get(4).and_then(|v| v.as_str()).unwrap_or_default();
    assert!(
        !opts.contains("start="),
        "a zero-offset play must use a plain loadfile, not start=0; options were {opts:?}"
    );
}
