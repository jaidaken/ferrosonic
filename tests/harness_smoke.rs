//! Smoke test for the integration harness.

mod common;

use common::TestDaemon;
use serde_json::Value;
use serial_test::serial;

#[tokio::test]
#[serial]
async fn fake_mpv_responds_to_basic_commands() {
    let td = TestDaemon::new().await;

    {
        let mut mpv = td.core.mpv.lock().await;
        mpv.pause().await.expect("fake mpv should accept pause");
    }

    let recorded = td.fake_mpv.commands().await;
    assert!(
        recorded.iter().any(|c| {
            c.first().and_then(Value::as_str) == Some("set_property")
                && c.get(1).and_then(Value::as_str) == Some("pause")
                && c.get(2).and_then(Value::as_bool) == Some(true)
        }),
        "fake mpv did not see the pause command; saw {:?}",
        recorded
    );
    assert!(
        td.fake_mpv.is_paused().await,
        "fake mpv state should be paused"
    );
}

#[tokio::test]
#[serial]
async fn fake_subsonic_serves_ping() {
    let td = TestDaemon::new().await;
    td.fake_subsonic.expect_ping().await;

    let url = format!("{}/rest/ping", td.fake_subsonic.url());
    let resp = reqwest::get(&url).await.expect("hit fake subsonic");
    assert_eq!(resp.status(), 200);
    let json: Value = resp.json().await.expect("parse subsonic-response json");
    assert_eq!(
        json["subsonic-response"]["status"], "ok",
        "fake subsonic should answer ping with status=ok"
    );
}

#[tokio::test]
#[serial]
async fn config_dir_override_isolates_tests() {
    let td = TestDaemon::new().await;
    let configured = std::env::var("FERROSONIC_CONFIG_DIR")
        .expect("test_daemon should set FERROSONIC_CONFIG_DIR");
    assert_eq!(
        configured,
        td.config_dir.path().to_string_lossy(),
        "FERROSONIC_CONFIG_DIR must point at the test's tempdir"
    );
}
