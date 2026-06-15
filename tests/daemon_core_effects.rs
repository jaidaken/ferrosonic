//! Daemon side-effect events: the core emits NowPlayingChanged, ConfigChanged,
//! and LibraryVersionChanged on the operations that should produce them. These
//! kill the "replace fn body with ()" mutants on the emit/broadcast helpers,
//! which survive when a test exercises the op but never observes the event.

mod common;

use std::time::Duration;

use common::{song, TestDaemon};
use ferrosonic::daemon::core::PlayMode;
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

/// First non-None projection of an event within 2s, else None.
async fn recv_value<T, F>(rx: &mut Receiver<DaemonEvent>, project: F) -> Option<T>
where
    F: Fn(&DaemonEvent) -> Option<T>,
{
    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    loop {
        match tokio::time::timeout_at(deadline, rx.recv()).await {
            Ok(Ok(ev)) => {
                if let Some(v) = project(&ev) {
                    return Some(v);
                }
            }
            _ => return None,
        }
    }
}

/// True if no event matching `pred` arrives within a short window.
async fn no_event_matching<F>(rx: &mut Receiver<DaemonEvent>, pred: F) -> bool
where
    F: Fn(&DaemonEvent) -> bool,
{
    let deadline = tokio::time::Instant::now() + Duration::from_millis(300);
    loop {
        match tokio::time::timeout_at(deadline, rx.recv()).await {
            Ok(Ok(ev)) => {
                if pred(&ev) {
                    return false;
                }
            }
            _ => return true,
        }
    }
}

#[tokio::test]
#[serial]
async fn quit_mpv_sends_quit_command_to_mpv() {
    let td = TestDaemon::new().await;
    {
        let mut s = td.state.write().await;
        s.queue = vec![song("s1", "A")];
        s.queue_position = Some(0);
    }
    // Play first so the mpv writer is connected; quit() is a no-op otherwise.
    td.core
        .play_queue_position(0, PlayMode::Direct)
        .await
        .unwrap();

    td.core.quit_mpv().await;

    let cmds = td.fake_mpv.commands().await;
    assert!(
        cmds.iter()
            .any(|c| c.first().and_then(|v| v.as_str()) == Some("quit")),
        "quit_mpv must send a quit command to mpv"
    );
}

#[tokio::test]
#[serial]
async fn star_without_a_refresh_does_not_emit_starred_changed() {
    // expect_star but no expect_starred: the post-toggle refresh fails, so the
    // `refreshed.is_some() && !stale` guard is false. `&&`->`||` would emit anyway.
    let td = TestDaemon::new().await;
    td.fake_subsonic.expect_star().await;
    {
        let mut s = td.state.write().await;
        s.queue = vec![song("s1", "A")];
    }
    let mut rx = td.core.subscribe();

    td.core.toggle_star_song("s1").await.unwrap();

    assert!(
        no_event_matching(&mut rx, |e| matches!(e, DaemonEvent::StarredChanged(_))).await,
        "without a successful refresh, StarredChanged must not be emitted"
    );
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
async fn refresh_artists_emits_library_version_changed_with_incremented_value() {
    // First bump of a fresh daemon emits version 1 (fetch_add returns the prior
    // 0, +1). The `+`->`*` mutant on bump_library_version would emit 0*1 = 0.
    let td = TestDaemon::new().await;
    td.fake_subsonic.expect_artists(&["The Cure"]).await;
    let mut rx = td.core.subscribe();
    td.core.refresh_artists().await;
    let version = recv_value(&mut rx, |e| match e {
        DaemonEvent::LibraryVersionChanged(v) => Some(*v),
        _ => None,
    })
    .await;
    assert_eq!(
        version,
        Some(1),
        "refresh_artists must emit the post-increment version (1), not the prior 0"
    );
}

#[tokio::test]
#[serial]
async fn auto_continue_with_no_random_songs_emits_an_error_notification() {
    // The `!s.is_empty()` guard mutated to `true` routes empty random songs
    // through the play arm, committing nothing and emitting no notification.
    let td = TestDaemon::new().await;
    td.fake_subsonic.expect_random_songs(&[]).await;
    {
        let mut s = td.state.write().await;
        s.queue = vec![song("s1", "A")];
        s.queue_position = Some(0);
        s.config.auto_continue = true;
        s.config.repeat_mode = ferrosonic::config::RepeatMode::Off;
    }
    let mut rx = td.core.subscribe();

    td.core.next_track().await.unwrap();

    assert!(
        recv_matching(&mut rx, |e| matches!(
            e,
            DaemonEvent::Notification { is_error: true, .. }
        ))
        .await,
        "exhausting the queue with no random songs must emit an error notification"
    );
    let s = td.state.read().await;
    assert_eq!(
        s.queue.len(),
        1,
        "no songs were appended, so the queue is unchanged"
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
