//! `toggle_star_song` against fake Subsonic /star + /unstar endpoints.

mod common;

use common::{song, TestDaemon};
use serial_test::serial;

#[tokio::test]
#[serial]
async fn unstarred_song_calls_star_endpoint() {
    let td = TestDaemon::new().await;
    td.fake_subsonic.expect_star().await;
    td.fake_subsonic.expect_starred().await;

    {
        let mut s = td.state.write().await;
        let mut sng = song("abc", "Hello");
        sng.starred = None;
        s.queue.push(sng);
    }

    let new_state = td.core.toggle_star_song("abc").await.unwrap();
    assert!(
        new_state,
        "toggle from unstarred should report starred=true"
    );

    let reqs = td.fake_subsonic.received_requests().await;
    assert!(
        reqs.iter().any(|r| r.url.path() == "/rest/star"),
        "expected /rest/star to be hit"
    );
    assert!(
        !reqs.iter().any(|r| r.url.path() == "/rest/unstar"),
        "unstar must not be called when toggling from unstarred"
    );
}

#[tokio::test]
#[serial]
async fn starred_song_calls_unstar_endpoint() {
    let td = TestDaemon::new().await;
    td.fake_subsonic.expect_unstar().await;
    td.fake_subsonic.expect_starred().await;

    {
        let mut s = td.state.write().await;
        let mut sng = song("abc", "Hello");
        sng.starred = Some("2026-05-11T00:00:00Z".into());
        s.queue.push(sng);
    }

    let new_state = td.core.toggle_star_song("abc").await.unwrap();
    assert!(
        !new_state,
        "toggle from starred should report starred=false"
    );

    let reqs = td.fake_subsonic.received_requests().await;
    assert!(
        reqs.iter().any(|r| r.url.path() == "/rest/unstar"),
        "expected /rest/unstar to be hit"
    );
}

#[tokio::test]
#[serial]
async fn toggle_passes_song_id_in_query() {
    let td = TestDaemon::new().await;
    td.fake_subsonic.expect_star().await;
    td.fake_subsonic.expect_starred().await;

    {
        let mut s = td.state.write().await;
        let mut sng = song("xyz-123", "Track");
        sng.starred = None;
        s.queue.push(sng);
    }

    td.core.toggle_star_song("xyz-123").await.unwrap();

    let reqs = td.fake_subsonic.received_requests().await;
    let star_req = reqs
        .iter()
        .find(|r| r.url.path() == "/rest/star")
        .expect("star endpoint hit");
    let query = star_req.url.query().unwrap_or_default();
    assert!(
        query.contains("id=xyz-123"),
        "song id must be url-encoded in query: got {}",
        query
    );
}
