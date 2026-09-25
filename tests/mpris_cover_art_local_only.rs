//! MPRIS publishes cover art only as a local file fetched through the daemon.
//! The TUI holds no password in daemon mode, so a server URL signed there is
//! an invalid login that trips the server's lockout and refuses streams.

mod common;

use std::sync::Arc;

use common::{song, TestDaemon};
use ferrosonic::app::state::{new_shared_client_state, new_shared_daemon_state, SharedDaemonState};
use ferrosonic::config::Config;
use ferrosonic::ipc::client::DaemonClient;
use ferrosonic::ipc::InProcessClient;
use ferrosonic::mpris::server::MprisPlayer;
use mpris_server::PlayerInterface;
use serial_test::serial;

/// The TUI's mirror as a daemon snapshot delivers it: server and user set, password stripped.
fn tui_mirror(base_url: &str) -> SharedDaemonState {
    let mut cfg = Config::new();
    cfg.base_url = base_url.to_string();
    cfg.username = "test".into();
    new_shared_daemon_state(cfg)
}

async fn play_song_with_cover(ds: &SharedDaemonState, cover: &str) {
    let mut s = ds.write().await;
    let mut sng = song("mf-1", "Track");
    sng.cover_art = Some(cover.to_string());
    s.queue.push(sng.clone());
    s.queue_position = Some(0);
    s.now_playing.song = Some(sng);
}

#[tokio::test]
#[serial]
async fn metadata_getter_never_carries_a_signed_server_url() {
    let ds = tui_mirror("https://example.com");
    play_song_with_cover(&ds, "mf-1").await;
    let client_state = new_shared_client_state(&Config::new());
    let player = MprisPlayer::new(ds, client_state, common::RecordingClient::new());

    let md = player.metadata().await.expect("metadata");

    assert_eq!(
        md.art_url().map(String::from),
        None,
        "with no local cover yet the getter publishes no art URL"
    );
}

#[tokio::test]
#[serial]
async fn cover_the_server_cannot_supply_publishes_no_art_url() {
    let td = TestDaemon::new().await;
    // No getCoverArt mock: the fake server answers 404 and the daemon replies with no bytes.
    let client: Arc<dyn DaemonClient> = Arc::new(InProcessClient::new(td.core.clone()));
    let ds = tui_mirror(&td.fake_subsonic.url());
    play_song_with_cover(&ds, "al-gone").await;
    let player = MprisPlayer::new(ds, new_shared_client_state(&Config::new()), client);

    assert_eq!(player.cover_file_uri("al-gone").await, None);
    assert_eq!(
        player
            .metadata()
            .await
            .expect("metadata")
            .art_url()
            .map(String::from),
        None,
        "an empty reply must not become an empty cover file"
    );
}

#[tokio::test]
#[serial]
async fn cover_reaches_mpris_as_a_local_file_fetched_by_the_daemon() {
    let td = TestDaemon::new().await;
    let png = vec![0x89, b'P', b'N', b'G', 1, 2, 3, 4];
    td.fake_subsonic
        .expect_get_cover_art("al-7", png.clone())
        .await;
    let client: Arc<dyn DaemonClient> = Arc::new(InProcessClient::new(td.core.clone()));
    let ds = tui_mirror(&td.fake_subsonic.url());
    play_song_with_cover(&ds, "al-7").await;
    let player = MprisPlayer::new(ds, new_shared_client_state(&Config::new()), client);

    let uri = player
        .cover_file_uri("al-7")
        .await
        .expect("the daemon fetch yields a local cover");
    let md = player.metadata().await.expect("metadata");

    let path = uri.strip_prefix("file://").expect("a file:// URI");
    assert_eq!(std::fs::read(path).expect("cover file readable"), png);
    assert_eq!(md.art_url().map(String::from), Some(uri));
    let clients: Vec<String> = td
        .fake_subsonic
        .received_requests()
        .await
        .iter()
        .filter(|r| r.url.path() == "/rest/getCoverArt")
        .map(|r| {
            r.url
                .query_pairs()
                .find(|(k, _)| k == "c")
                .map(|(_, v)| v.into_owned())
                .unwrap_or_default()
        })
        .collect();
    assert_eq!(
        clients,
        ["ferrosonic-rs"],
        "exactly one cover request, signed by the daemon's client"
    );
}
