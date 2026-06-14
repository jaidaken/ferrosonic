//! Daemon side-effect events: the core emits NowPlayingChanged, ConfigChanged,
//! and LibraryVersionChanged on the operations that should produce them. These
//! kill the "replace fn body with ()" mutants on the emit/broadcast helpers,
//! which survive when a test exercises the op but never observes the event.

mod common;

use std::time::Duration;

use common::{song, TestDaemon};
use ferrosonic::ipc::DaemonEvent;
use serial_test::serial;
use tokio::sync::broadcast::Receiver;

async fn recv_matching<F>(rx: &mut Receiver<DaemonEvent>, pred: F) -> bool
where
    F: Fn(&DaemonEvent) -> bool,
{
    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    loop {
        match tokio::time::timeout_at(deadline, rx.recv()).await {
            Ok(Ok(ev)) => {
                if pred(&ev) {
                    return true;
                }
            }
            _ => return false,
        }
    }
}

#[tokio::test]
#[serial]
async fn broadcast_now_playing_emits_now_playing_changed() {
    let td = TestDaemon::new().await;
    let mut rx = td.core.subscribe();
    td.core.broadcast_now_playing().await;
    assert!(
        recv_matching(&mut rx, |e| matches!(e, DaemonEvent::NowPlayingChanged(_))).await,
        "broadcast_now_playing must emit NowPlayingChanged"
    );
}

#[tokio::test]
#[serial]
async fn refresh_artists_emits_library_version_changed() {
    let td = TestDaemon::new().await;
    td.fake_subsonic.expect_artists(&["The Cure"]).await;
    let mut rx = td.core.subscribe();
    td.core.refresh_artists().await;
    assert!(
        recv_matching(&mut rx, |e| matches!(e, DaemonEvent::LibraryVersionChanged(_))).await,
        "refresh_artists must bump and emit LibraryVersionChanged"
    );
}

#[tokio::test]
#[serial]
async fn refresh_playlists_emits_playlists_changed() {
    let td = TestDaemon::new().await;
    td.fake_subsonic.expect_playlists().await;
    let mut rx = td.core.subscribe();
    td.core.refresh_playlists().await;
    assert!(
        recv_matching(&mut rx, |e| matches!(e, DaemonEvent::PlaylistsChanged(_))).await,
        "refresh_playlists must emit PlaylistsChanged"
    );
}

#[tokio::test]
#[serial]
async fn refresh_starred_emits_starred_changed() {
    let td = TestDaemon::new().await;
    td.fake_subsonic.expect_starred().await;
    let mut rx = td.core.subscribe();
    td.core.refresh_starred().await;
    assert!(
        recv_matching(&mut rx, |e| matches!(e, DaemonEvent::StarredChanged(_))).await,
        "refresh_starred must emit StarredChanged"
    );
}

#[tokio::test]
#[serial]
async fn refresh_random_emits_random_changed() {
    let td = TestDaemon::new().await;
    td.fake_subsonic.expect_random_songs(&[]).await;
    let mut rx = td.core.subscribe();
    td.core.refresh_random().await;
    assert!(
        recv_matching(&mut rx, |e| matches!(e, DaemonEvent::RandomChanged(_))).await,
        "refresh_random must emit RandomChanged"
    );
}

#[tokio::test]
#[serial]
async fn star_toggle_emits_starred_changed_and_commits_server_list() {
    let td = TestDaemon::new().await;
    td.fake_subsonic.expect_star().await;
    td.fake_subsonic.expect_starred().await; // fake returns an empty starred list
    {
        let mut s = td.state.write().await;
        s.queue = vec![song("s1", "A")];
    }
    let mut rx = td.core.subscribe();

    td.core.toggle_star_song("s1").await.unwrap();

    assert!(
        recv_matching(&mut rx, |e| matches!(e, DaemonEvent::StarredChanged(_))).await,
        "a successful star with a refresh must emit StarredChanged"
    );
    // The server list (empty) must overwrite the optimistic [s1]; the `if !stale`
    // commit mutant would leave the optimistic entry.
    let s = td.state.read().await;
    assert!(
        s.library.starred_songs.is_empty(),
        "the refreshed server starred list is committed over the optimistic update"
    );
}

#[tokio::test]
#[serial]
async fn set_cava_enabled_emits_config_changed() {
    let td = TestDaemon::new().await;
    let mut rx = td.core.subscribe();
    td.core.set_cava_enabled(true).await.unwrap();
    assert!(
        recv_matching(&mut rx, |e| matches!(e, DaemonEvent::ConfigChanged(_))).await,
        "set_cava_enabled must emit ConfigChanged"
    );
}

