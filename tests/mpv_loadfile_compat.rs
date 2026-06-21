//! GitHub issue #30: mpv < 0.38 has no loadfile insertion-index argument, so
//! the 5-arg loadfile used for resume-at-offset is rejected "invalid
//! parameter" and the track skips. Resume must fall back to a 3-arg load plus
//! a post-load absolute seek on old mpv, and keep the 5-arg start= on >= 0.38.

mod common;

use common::{song, TestDaemon};
use ferrosonic::daemon::core::PlayMode;
use serial_test::serial;

async fn seed_and_pause_at_30(td: &TestDaemon) {
    {
        let mut s = td.state.write().await;
        s.queue = vec![song("s1", "Track One"), song("s2", "Track Two")];
        s.queue_position = Some(0);
    }
    td.core
        .play_queue_position(0, PlayMode::Direct)
        .await
        .unwrap();
    {
        let mut s = td.state.write().await;
        s.now_playing.position = 30.0;
    }
    td.core.pause_playback().await.unwrap();
}

fn is_replace_loadfile(c: &[serde_json::Value]) -> bool {
    c.first().and_then(|v| v.as_str()) == Some("loadfile")
        && c.get(2).and_then(|v| v.as_str()) == Some("replace")
}

#[tokio::test]
#[serial]
async fn resume_on_mpv_below_0_38_uses_seek_not_five_arg_loadfile() {
    let td = TestDaemon::new_with_mpv_version("0.35.0").await;
    seed_and_pause_at_30(&td).await;
    td.core.resume_playback().await.unwrap();

    // The offset is restored by a post-load absolute seek issued by the
    // spawned settle task, since the 5-arg start= form is unavailable.
    let seeked = td
        .fake_mpv
        .wait_for(2000, |cmds| {
            cmds.iter().any(|c| {
                c.first().and_then(|v| v.as_str()) == Some("seek")
                    && c.get(1).and_then(|v| v.as_f64()) == Some(30.0)
                    && c.get(2).and_then(|v| v.as_str()) == Some("absolute")
            })
        })
        .await;
    assert!(
        seeked,
        "resume on mpv 0.35 must seek to 30s after a 3-arg load (issue #30)"
    );

    let cmds = td.fake_mpv.commands().await;
    let resume_reload = cmds
        .iter()
        .rev()
        .find(|c| is_replace_loadfile(c))
        .expect("a replace loadfile");
    assert!(
        resume_reload.get(3).is_none(),
        "mpv < 0.38 resume must use a 3-arg loadfile (no index arg), got {resume_reload:?}"
    );
    assert!(
        td.fake_mpv.loaded_file().await.is_some(),
        "the track must load (not be rejected) on old mpv"
    );
    assert!(
        (td.fake_mpv.position().await - 30.0).abs() < 0.01,
        "the playhead must land at the 30s offset"
    );
}

#[tokio::test]
#[serial]
async fn resume_on_mpv_0_38_plus_keeps_five_arg_start() {
    let td = TestDaemon::new_with_mpv_version("0.41.0").await;
    seed_and_pause_at_30(&td).await;
    td.core.resume_playback().await.unwrap();

    let cmds = td.fake_mpv.commands().await;
    let resume_reload = cmds
        .iter()
        .rev()
        .find(|c| is_replace_loadfile(c))
        .expect("a replace loadfile");
    let opts = resume_reload
        .get(4)
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    assert!(
        opts.contains("start=30"),
        "mpv >= 0.38 keeps the decode-from-offset start= form, got {resume_reload:?}"
    );
    let seek_30 = cmds.iter().any(|c| {
        c.first().and_then(|v| v.as_str()) == Some("seek")
            && c.get(1).and_then(|v| v.as_f64()) == Some(30.0)
    });
    assert!(
        !seek_30,
        "the modern path decodes from the offset, so it issues no post-load seek to 30s"
    );
}

#[tokio::test]
#[serial]
async fn snapshot_reports_mpv_version_for_the_advisory() {
    let old = TestDaemon::new_with_mpv_version("0.35.0").await;
    assert_eq!(
        old.core.snapshot().await.mpv_version,
        Some((0, 35)),
        "snapshot must carry the detected version so the TUI can advise"
    );
    let new = TestDaemon::new_with_mpv_version("0.41.0").await;
    assert_eq!(new.core.snapshot().await.mpv_version, Some((0, 41)));
}

#[tokio::test]
#[serial]
async fn resume_with_non_finite_position_emits_no_malformed_start() {
    let td = TestDaemon::new().await;
    {
        let mut s = td.state.write().await;
        s.queue = vec![song("s1", "Track One")];
        s.queue_position = Some(0);
    }
    td.core
        .play_queue_position(0, PlayMode::Direct)
        .await
        .unwrap();
    {
        let mut s = td.state.write().await;
        // +inf passes the `> 0.0` gate (unlike NaN), so without a finite guard
        // it reaches `format!("start={}")` and emits start=inf.
        s.now_playing.position = f64::INFINITY;
    }
    td.core.pause_playback().await.unwrap();
    td.core.resume_playback().await.unwrap();

    let cmds = td.fake_mpv.commands().await;
    for c in &cmds {
        let opts = c.get(4).and_then(|v| v.as_str()).unwrap_or_default();
        assert!(
            !opts.contains("NaN") && !opts.contains("inf"),
            "a non-finite saved position must never emit start=NaN/inf, got {c:?}"
        );
        if c.first().and_then(|v| v.as_str()) == Some("seek") {
            let target = c.get(1).and_then(|v| v.as_f64()).unwrap_or(0.0);
            assert!(target.is_finite(), "seek target must be finite, got {c:?}");
        }
    }
}
