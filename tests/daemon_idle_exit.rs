//! The daemon exits when it has no connected clients and playback is stopped,
//! so a daemon spawned for a TUI that has gone away never stays orphaned.

mod common;

use std::time::Duration;

use common::TestDaemon;
use ferrosonic::daemon::state::PlaybackState;
use serial_test::serial;

#[tokio::test]
#[serial]
async fn idle_only_when_no_clients_and_stopped() {
    let td = TestDaemon::new().await;
    assert!(
        td.core.is_idle_for_exit().await,
        "a fresh daemon with no clients and stopped playback is idle"
    );

    let guard = td.core.client_guard();
    assert!(
        !td.core.is_idle_for_exit().await,
        "a connected client keeps the daemon alive"
    );
    drop(guard);
    assert!(
        td.core.is_idle_for_exit().await,
        "dropping the last client returns to idle"
    );

    {
        let mut s = td.state.write().await;
        s.now_playing.state = PlaybackState::Playing;
    }
    assert!(
        !td.core.is_idle_for_exit().await,
        "active playback keeps the daemon alive even with no clients"
    );
}

#[tokio::test(start_paused = true)]
#[serial]
async fn idle_monitor_requests_shutdown_after_the_grace_period() {
    let td = TestDaemon::new().await;
    let handle = td.core.spawn_idle_exit_monitor();

    // Drive the periodic 15s idle checks: yield so the task registers its sleep,
    // then advance past it. 30s grace = 2 idle ticks; loop with margin.
    for _ in 0..5 {
        tokio::task::yield_now().await;
        tokio::time::advance(Duration::from_secs(15)).await;
    }

    let joined = tokio::time::timeout(Duration::from_secs(1), handle).await;
    assert!(joined.is_ok(), "the monitor task must finish after requesting shutdown");
    assert!(
        tokio::time::timeout(Duration::from_secs(1), td.core.shutdown_signal())
            .await
            .is_ok(),
        "an idle daemon must request shutdown after the grace period"
    );
}

#[tokio::test(start_paused = true)]
#[serial]
async fn idle_monitor_does_not_shut_down_while_a_client_is_connected() {
    let td = TestDaemon::new().await;
    let _client = td.core.client_guard();
    let handle = td.core.spawn_idle_exit_monitor();

    tokio::time::advance(Duration::from_secs(120)).await;

    assert!(
        tokio::time::timeout(Duration::from_secs(1), td.core.shutdown_signal())
            .await
            .is_err(),
        "the daemon must stay up while a client is connected"
    );
    handle.abort();
}
