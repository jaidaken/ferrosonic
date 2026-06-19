//! Dual-path scrobble reporting driven through the daemon scrobble tick.

mod common;

use common::TestDaemon;
use ferrosonic::daemon::state::PlaybackState;
use serial_test::serial;
use std::time::Duration;

fn song(id: &str) -> ferrosonic::subsonic::models::Child {
    common::song(id, id)
}

/// Bounded poll until the fake server has seen a request to `path`.
async fn wait_for(td: &TestDaemon, path: &str) -> bool {
    for _ in 0..200 {
        if td
            .fake_subsonic
            .received_requests()
            .await
            .iter()
            .any(|r| r.url.path() == path)
        {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    false
}

/// Wait out the constructor's capability auto-detect so a test can then force
/// the path it wants without racing the background store.
async fn settle_capability(td: &TestDaemon) {
    wait_for(td, "/rest/getOpenSubsonicExtensions").await;
}

async fn set_now_playing(td: &TestDaemon, id: &str, state: PlaybackState, pos: f64, dur: f64) {
    let mut s = td.state.write().await;
    s.now_playing.song = Some(song(id));
    s.now_playing.state = state;
    s.now_playing.position = pos;
    s.now_playing.duration = dur;
}

async fn query(td: &TestDaemon, path: &str) -> Vec<String> {
    td.fake_subsonic
        .received_requests()
        .await
        .into_iter()
        .filter(|r| r.url.path() == path)
        .map(|r| r.url.query().unwrap_or_default().to_string())
        .collect()
}

#[tokio::test]
#[serial]
async fn classic_start_sends_now_playing() {
    let td = TestDaemon::new().await;
    settle_capability(&td).await;
    td.core.set_playback_report_for_test(false);
    td.fake_subsonic.expect_scrobble().await;

    set_now_playing(&td, "s1", PlaybackState::Playing, 0.0, 300.0).await;
    td.core.scrobble_tick().await;

    assert!(
        wait_for(&td, "/rest/scrobble").await,
        "scrobble request sent"
    );
    let qs = query(&td, "/rest/scrobble").await;
    assert!(
        qs.iter()
            .any(|q| q.contains("id=s1") && q.contains("submission=false")),
        "now-playing scrobble (submission=false); saw {qs:?}"
    );
}

#[tokio::test]
#[serial]
async fn classic_submits_after_threshold() {
    let td = TestDaemon::new().await;
    settle_capability(&td).await;
    td.core.set_playback_report_for_test(false);
    td.fake_subsonic.expect_scrobble().await;

    // Start, then advance past half of a 300s track (>= 150s).
    set_now_playing(&td, "s1", PlaybackState::Playing, 0.0, 300.0).await;
    td.core.scrobble_tick().await;
    set_now_playing(&td, "s1", PlaybackState::Playing, 160.0, 300.0).await;
    td.core.scrobble_tick().await;

    assert!(
        wait_for(&td, "/rest/scrobble").await,
        "scrobble request sent"
    );
    let qs = query(&td, "/rest/scrobble").await;
    assert!(
        qs.iter()
            .any(|q| q.contains("id=s1") && q.contains("submission=true")),
        "played-submission scrobble (submission=true); saw {qs:?}"
    );
}

#[tokio::test]
#[serial]
async fn modern_reports_playing_on_start() {
    let td = TestDaemon::new().await;
    settle_capability(&td).await;
    td.core.set_playback_report_for_test(true);
    td.fake_subsonic.expect_report_playback().await;

    set_now_playing(&td, "s1", PlaybackState::Playing, 0.0, 300.0).await;
    td.core.scrobble_tick().await;

    assert!(
        wait_for(&td, "/rest/reportPlayback").await,
        "reportPlayback request sent on the modern path"
    );
    let qs = query(&td, "/rest/reportPlayback").await;
    assert!(
        qs.iter()
            .any(|q| q.contains("mediaId=s1") && q.contains("state=playing")),
        "reportPlayback state=playing; saw {qs:?}"
    );
}

#[tokio::test]
#[serial]
async fn modern_reports_stopped_when_playback_ends() {
    let td = TestDaemon::new().await;
    settle_capability(&td).await;
    td.core.set_playback_report_for_test(true);
    td.fake_subsonic.expect_report_playback().await;

    // Play, then clear the track (stop / end of queue).
    set_now_playing(&td, "s1", PlaybackState::Playing, 100.0, 300.0).await;
    td.core.scrobble_tick().await;
    {
        let mut s = td.state.write().await;
        s.now_playing.song = None;
        s.now_playing.state = PlaybackState::Stopped;
        s.now_playing.position = 0.0;
    }
    td.core.scrobble_tick().await;

    assert!(
        wait_for(&td, "/rest/reportPlayback").await,
        "reportPlayback sent"
    );
    let qs = query(&td, "/rest/reportPlayback").await;
    assert!(
        qs.iter()
            .any(|q| q.contains("mediaId=s1") && q.contains("state=stopped")),
        "a stopped/cleared track reports state=stopped so the server scrobbles it; saw {qs:?}"
    );
}

#[tokio::test]
#[serial]
async fn classic_does_not_resubmit_a_track_first_seen_past_threshold() {
    let td = TestDaemon::new().await;
    settle_capability(&td).await;
    td.core.set_playback_report_for_test(false);
    td.fake_subsonic.expect_scrobble().await;

    // First observation already past 50% (scrobbling re-enabled mid-play); a
    // played-submission must NOT fire, or the play double-counts.
    set_now_playing(&td, "s1", PlaybackState::Playing, 200.0, 300.0).await;
    td.core.scrobble_tick().await;
    set_now_playing(&td, "s1", PlaybackState::Playing, 260.0, 300.0).await;
    td.core.scrobble_tick().await;
    tokio::time::sleep(Duration::from_millis(80)).await;

    let qs = query(&td, "/rest/scrobble").await;
    assert!(
        !qs.iter().any(|q| q.contains("submission=true")),
        "no played-submission for a track first seen past threshold; saw {qs:?}"
    );
}

#[tokio::test]
#[serial]
async fn disabled_config_sends_nothing() {
    let td = TestDaemon::new().await;
    settle_capability(&td).await;
    td.core.set_playback_report_for_test(false);
    td.fake_subsonic.expect_scrobble().await;
    td.fake_subsonic.expect_report_playback().await;
    {
        let mut s = td.state.write().await;
        s.config.scrobble = false;
    }

    set_now_playing(&td, "s1", PlaybackState::Playing, 200.0, 300.0).await;
    td.core.scrobble_tick().await;
    tokio::time::sleep(Duration::from_millis(80)).await;

    let scrobbles = query(&td, "/rest/scrobble").await;
    let reports = query(&td, "/rest/reportPlayback").await;
    assert!(
        scrobbles.is_empty() && reports.is_empty(),
        "scrobbling off must send nothing; scrobble={scrobbles:?} report={reports:?}"
    );
}
