//! `InProcessClient::enqueue_songs` Replace/Append/InsertAfter coverage.

mod common;

use common::{song, songs, TestDaemon};
use ferrosonic::ipc::client::{DaemonClient, InProcessClient};
use ferrosonic::ipc::protocol::{DaemonRequest, DaemonResponse, EnqueueMode};
use serde_json::Value;
use serial_test::serial;

#[tokio::test]
#[serial]
async fn replace_overwrites_queue_and_starts_playback_when_play_from_set() {
    let td = TestDaemon::new().await;
    td.fake_subsonic.expect_ping().await;
    let client = InProcessClient::new(td.core.clone());

    let resp = client
        .request(DaemonRequest::EnqueueSongs {
            songs: songs("seed", 3),
            mode: EnqueueMode::Replace { play_from: Some(0) },
        })
        .await
        .expect("enqueue Replace play_from=0");

    assert!(matches!(resp, DaemonResponse::Ok));

    let saw_loadfile = td
        .fake_mpv
        .wait_for(2000, |cmds| {
            cmds.iter()
                .any(|c| c.first().and_then(Value::as_str) == Some("loadfile"))
        })
        .await;
    assert!(
        saw_loadfile,
        "Replace with play_from must call mpv loadfile"
    );

    let s = td.state.read().await;
    assert_eq!(s.queue.len(), 3);
    assert_eq!(s.queue[0].id, "seed-0");
}

#[tokio::test]
#[serial]
async fn replace_with_no_play_from_clears_and_emits_but_does_not_play() {
    let td = TestDaemon::new().await;
    let client = InProcessClient::new(td.core.clone());

    {
        let mut s = td.state.write().await;
        s.queue = vec![song("old", "Old Track")];
    }

    client
        .request(DaemonRequest::EnqueueSongs {
            songs: songs("new", 2),
            mode: EnqueueMode::Replace { play_from: None },
        })
        .await
        .unwrap();

    let s = td.state.read().await;
    assert_eq!(s.queue.len(), 2);
    assert_eq!(s.queue[0].id, "new-0");
    assert_eq!(s.queue_position, None);

    let no_loadfile = !td
        .fake_mpv
        .commands()
        .await
        .iter()
        .any(|c| c.first().and_then(Value::as_str) == Some("loadfile"));
    assert!(
        no_loadfile,
        "Replace without play_from must not call loadfile"
    );
}

#[tokio::test]
#[serial]
async fn append_preserves_existing_queue() {
    let td = TestDaemon::new().await;
    let client = InProcessClient::new(td.core.clone());

    {
        let mut s = td.state.write().await;
        s.queue = vec![song("a", "A"), song("b", "B")];
        s.queue_position = Some(1);
    }

    client
        .request(DaemonRequest::EnqueueSongs {
            songs: vec![song("c", "C"), song("d", "D")],
            mode: EnqueueMode::Append,
        })
        .await
        .unwrap();

    let s = td.state.read().await;
    assert_eq!(
        s.queue.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(),
        vec!["a", "b", "c", "d"]
    );
    assert_eq!(
        s.queue_position,
        Some(1),
        "Append must not move queue_position"
    );
}

#[tokio::test]
#[serial]
async fn insert_after_places_songs_at_position_plus_one() {
    let td = TestDaemon::new().await;
    let client = InProcessClient::new(td.core.clone());

    {
        let mut s = td.state.write().await;
        s.queue = vec![song("a", "A"), song("b", "B"), song("c", "C")];
        s.queue_position = Some(0);
    }

    client
        .request(DaemonRequest::EnqueueSongs {
            songs: vec![song("x", "X"), song("y", "Y")],
            mode: EnqueueMode::InsertAfter(0),
        })
        .await
        .unwrap();

    let s = td.state.read().await;
    assert_eq!(
        s.queue.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(),
        vec!["a", "x", "y", "b", "c"]
    );
}

#[tokio::test]
#[serial]
async fn insert_after_past_end_appends_safely() {
    let td = TestDaemon::new().await;
    let client = InProcessClient::new(td.core.clone());

    {
        let mut s = td.state.write().await;
        s.queue = vec![song("a", "A")];
    }

    client
        .request(DaemonRequest::EnqueueSongs {
            songs: vec![song("b", "B")],
            mode: EnqueueMode::InsertAfter(99),
        })
        .await
        .unwrap();

    let s = td.state.read().await;
    assert_eq!(
        s.queue.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(),
        vec!["a", "b"]
    );
}

#[tokio::test]
#[serial]
async fn replace_with_empty_song_list_clears_queue() {
    let td = TestDaemon::new().await;
    let client = InProcessClient::new(td.core.clone());

    {
        let mut s = td.state.write().await;
        s.queue = songs("old", 3);
        s.queue_position = Some(1);
    }

    client
        .request(DaemonRequest::EnqueueSongs {
            songs: vec![],
            mode: EnqueueMode::Replace { play_from: None },
        })
        .await
        .unwrap();

    let s = td.state.read().await;
    assert!(s.queue.is_empty());
    assert_eq!(s.queue_position, None);
}
