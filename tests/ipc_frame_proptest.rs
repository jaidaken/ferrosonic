//! Property tests for IPC frame round-trip across many variants.

use ferrosonic::ipc::frame::{read_frame, write_frame, Frame};
use ferrosonic::ipc::protocol::{DaemonRequest, DaemonResponse, EnqueueMode};
use proptest::prelude::*;

fn arb_enqueue_mode() -> impl Strategy<Value = EnqueueMode> {
    prop_oneof![
        any::<Option<usize>>().prop_map(|play_from| EnqueueMode::Replace { play_from }),
        Just(EnqueueMode::Append),
        (0usize..10000).prop_map(EnqueueMode::InsertAfter),
    ]
}

fn arb_request() -> impl Strategy<Value = DaemonRequest> {
    prop_oneof![
        Just(DaemonRequest::Pause),
        Just(DaemonRequest::Resume),
        Just(DaemonRequest::TogglePause),
        Just(DaemonRequest::Stop),
        any::<f64>().prop_map(DaemonRequest::Seek),
        any::<f64>().prop_map(DaemonRequest::SeekRelative),
        Just(DaemonRequest::Next),
        Just(DaemonRequest::Previous),
        any::<i32>().prop_map(DaemonRequest::SetVolume),
        (0usize..10000).prop_map(DaemonRequest::PlayQueueIndex),
        (0usize..10000).prop_map(DaemonRequest::RemoveFromQueue),
        Just(DaemonRequest::ClearQueue),
        Just(DaemonRequest::ShuffleQueue),
        Just(DaemonRequest::ShuffleLibrary),
        (0usize..10000, 0usize..10000)
            .prop_map(|(from, to)| DaemonRequest::MoveQueueItem { from, to }),
        Just(DaemonRequest::ClearQueueHistory),
        Just(DaemonRequest::RefreshStarred),
        Just(DaemonRequest::RefreshRandom),
        Just(DaemonRequest::RefreshArtists),
        Just(DaemonRequest::RefreshPlaylists),
        any::<String>().prop_map(DaemonRequest::ToggleStarSong),
        any::<String>().prop_map(DaemonRequest::LoadArtist),
        any::<String>().prop_map(DaemonRequest::LoadAlbum),
        any::<String>().prop_map(DaemonRequest::LoadPlaylist),
        any::<bool>().prop_map(DaemonRequest::SetCavaEnabled),
        any::<u8>().prop_map(DaemonRequest::SetCavaSize),
        any::<bool>().prop_map(DaemonRequest::SetDaemonEnabled),
        any::<bool>().prop_map(DaemonRequest::SetAutoContinue),
        any::<bool>().prop_map(DaemonRequest::SetCoverArtEnabled),
        any::<u8>().prop_map(DaemonRequest::SetCoverArtSize),
        Just(DaemonRequest::Subscribe),
        Just(DaemonRequest::Snapshot),
        Just(DaemonRequest::Shutdown),
        Just(DaemonRequest::Ping),
    ]
}

fn arb_response() -> impl Strategy<Value = DaemonResponse> {
    prop_oneof![
        Just(DaemonResponse::Ok),
        any::<String>().prop_map(DaemonResponse::Err),
        (any::<bool>(), any::<String>())
            .prop_map(|(ok, message)| DaemonResponse::ConnectionTestResult { ok, message }),
        (0usize..100000).prop_map(DaemonResponse::HistoryCleared),
        any::<Vec<u8>>().prop_map(DaemonResponse::CoverArt),
        Just(DaemonResponse::Pong),
    ]
}

async fn roundtrip_request(req: DaemonRequest) -> Frame {
    let frame = Frame::Request { id: 42, req };
    let mut buf: Vec<u8> = Vec::new();
    write_frame(&mut buf, &frame).await.unwrap();
    let mut reader = buf.as_slice();
    read_frame(&mut reader).await.unwrap()
}

async fn roundtrip_response(resp: DaemonResponse) -> Frame {
    let frame = Frame::Response {
        id: 7,
        payload: Ok(resp),
    };
    let mut buf: Vec<u8> = Vec::new();
    write_frame(&mut buf, &frame).await.unwrap();
    let mut reader = buf.as_slice();
    read_frame(&mut reader).await.unwrap()
}

#[test]
fn arbitrary_request_round_trips_preserve_id() {
    let mut runner = proptest::test_runner::TestRunner::default();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runner
        .run(&arb_request(), |req| {
            let decoded = rt.block_on(roundtrip_request(req));
            match decoded {
                Frame::Request { id, .. } => prop_assert_eq!(id, 42),
                _ => prop_assert!(false, "expected Request"),
            }
            Ok(())
        })
        .unwrap();
}

#[test]
fn arbitrary_response_round_trips_preserve_id() {
    let mut runner = proptest::test_runner::TestRunner::default();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runner
        .run(&arb_response(), |resp| {
            let decoded = rt.block_on(roundtrip_response(resp));
            match decoded {
                Frame::Response { id, .. } => prop_assert_eq!(id, 7),
                _ => prop_assert!(false, "expected Response"),
            }
            Ok(())
        })
        .unwrap();
}

#[test]
fn enqueue_mode_round_trips_through_serde() {
    let mut runner = proptest::test_runner::TestRunner::default();
    runner
        .run(&arb_enqueue_mode(), |mode| {
            let bytes = serde_json::to_vec(&mode).unwrap();
            let back: EnqueueMode = serde_json::from_slice(&bytes).unwrap();
            match (&mode, &back) {
                (EnqueueMode::Replace { play_from: a }, EnqueueMode::Replace { play_from: b }) => {
                    prop_assert_eq!(a, b)
                }
                (EnqueueMode::Append, EnqueueMode::Append) => {}
                (EnqueueMode::InsertAfter(a), EnqueueMode::InsertAfter(b)) => {
                    prop_assert_eq!(a, b)
                }
                (a, b) => prop_assert!(false, "variant mismatch: {:?} vs {:?}", a, b),
            }
            Ok(())
        })
        .unwrap();
}
