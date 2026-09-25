//! What `update_mpris_properties` announces to MPRIS clients, recorded without D-Bus:
//! playback state first, then track metadata carrying the local cover file.

mod common;

use std::sync::{Arc, Mutex};

use common::{song, TestDaemon};
use ferrosonic::app::state::{new_shared_client_state, new_shared_daemon_state, SharedDaemonState};
use ferrosonic::config::Config;
use ferrosonic::daemon::state::PlaybackState;
use ferrosonic::ipc::client::DaemonClient;
use ferrosonic::ipc::InProcessClient;
use ferrosonic::mpris::server::{update_mpris_properties, MprisPlayer, PropertySink};
use mpris_server::{PlaybackStatus, Property};
use serial_test::serial;

struct Recorder {
    player: MprisPlayer,
    pushed: Mutex<Vec<Vec<Property>>>,
}

impl PropertySink for Recorder {
    fn player(&self) -> &MprisPlayer {
        &self.player
    }

    async fn push(&self, props: Vec<Property>) -> mpris_server::zbus::Result<()> {
        self.pushed.lock().expect("recorder lock").push(props);
        Ok(())
    }
}

fn tui_mirror(base_url: &str) -> SharedDaemonState {
    let mut cfg = Config::new();
    cfg.base_url = base_url.to_string();
    cfg.username = "test".into();
    new_shared_daemon_state(cfg)
}

#[tokio::test]
#[serial]
async fn playing_track_pushes_state_then_metadata_with_the_local_cover() {
    let td = TestDaemon::new().await;
    let png = vec![0x89, b'P', b'N', b'G', 9, 8, 7];
    td.fake_subsonic
        .expect_get_cover_art("al-7", png.clone())
        .await;
    let client: Arc<dyn DaemonClient> = Arc::new(InProcessClient::new(td.core.clone()));
    let ds = tui_mirror(&td.fake_subsonic.url());
    {
        let mut s = ds.write().await;
        let mut first = song("t-1", "First Track");
        first.cover_art = Some("al-7".into());
        s.queue = vec![first.clone(), song("t-2", "Second Track")];
        s.queue_position = Some(0);
        s.now_playing.song = Some(first);
        s.now_playing.state = PlaybackState::Playing;
    }
    let recorder = Recorder {
        player: MprisPlayer::new(ds.clone(), new_shared_client_state(&Config::new()), client),
        pushed: Mutex::new(Vec::new()),
    };

    update_mpris_properties(&recorder, &ds)
        .await
        .expect("push succeeds");

    let pushed = recorder.pushed.lock().expect("recorder lock").clone();
    assert_eq!(pushed.len(), 2, "state batch, then metadata batch");
    assert_eq!(
        pushed[0],
        [
            Property::PlaybackStatus(PlaybackStatus::Playing),
            Property::CanGoNext(true),
            Property::CanGoPrevious(false),
            Property::CanPlay(true),
        ]
    );
    let [Property::Metadata(md)] = pushed[1].as_slice() else {
        panic!("second batch is one Metadata property, got {:?}", pushed[1]);
    };
    assert_eq!(md.title(), Some("First Track"));
    let art = md.art_url().expect("metadata carries the cover");
    let path = art.strip_prefix("file://").expect("a local file URL");
    assert_eq!(std::fs::read(path).expect("cover file readable"), png);
}

#[tokio::test]
#[serial]
async fn empty_queue_pushes_stopped_state_and_no_metadata() {
    let ds = tui_mirror("https://example.com");
    let recorder = Recorder {
        player: MprisPlayer::new(
            ds.clone(),
            new_shared_client_state(&Config::new()),
            common::RecordingClient::new(),
        ),
        pushed: Mutex::new(Vec::new()),
    };

    update_mpris_properties(&recorder, &ds)
        .await
        .expect("push succeeds");

    let pushed = recorder.pushed.lock().expect("recorder lock").clone();
    assert_eq!(
        pushed,
        [vec![
            Property::PlaybackStatus(PlaybackStatus::Stopped),
            Property::CanGoNext(false),
            Property::CanGoPrevious(false),
            Property::CanPlay(false),
        ]]
    );
}
