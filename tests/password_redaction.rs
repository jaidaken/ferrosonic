//! Secret newtype redaction contract: Debug + Serialize mask by default; wire helpers reveal; Drop zeroizes; IPC password handshake round-trips plaintext on the wire but masks in Debug.

use ferrosonic::ipc::frame::{read_frame, write_frame, Frame};
use ferrosonic::ipc::protocol::DaemonRequest;
use ferrosonic::secret::Secret;
use serde::{Deserialize, Serialize};

#[test]
fn debug_masks_nonempty_secret() {
    let s = Secret::from_string("hunter2".to_string());
    let d = format!("{:?}", s);
    assert!(!d.contains("hunter2"), "Debug must not contain plaintext");
    assert!(d.contains("***"), "Debug must contain mask");
}

#[test]
fn debug_empty_secret_does_not_say_stars() {
    let s = Secret::new();
    let d = format!("{:?}", s);
    assert!(!d.contains("***"));
}

#[test]
fn default_serialize_masks_nonempty() {
    let s = Secret::from_string("hunter2".to_string());
    let j = serde_json::to_string(&s).unwrap();
    assert_eq!(j, "\"***\"");
}

#[test]
fn default_serialize_empty_stays_empty() {
    let s = Secret::new();
    let j = serde_json::to_string(&s).unwrap();
    assert_eq!(j, "\"\"");
}

#[test]
fn deserialize_accepts_plaintext_and_round_trips_via_reveal() {
    let s: Secret = serde_json::from_str("\"hunter2\"").unwrap();
    assert_eq!(s.reveal(), "hunter2");
}

#[test]
fn wire_helper_serializes_revealed_plaintext() {
    #[derive(Serialize, Deserialize)]
    struct W {
        #[serde(
            serialize_with = "ferrosonic::secret::serialize_revealed",
            deserialize_with = "ferrosonic::secret::deserialize_secret"
        )]
        p: Secret,
    }
    let w = W {
        p: Secret::from_string("hunter2".to_string()),
    };
    let j = serde_json::to_string(&w).unwrap();
    assert_eq!(j, "{\"p\":\"hunter2\"}");
    let back: W = serde_json::from_str(&j).unwrap();
    assert_eq!(back.p.reveal(), "hunter2");
}

#[test]
fn clear_empties_the_secret() {
    let mut s = Secret::from_string("hunter2".to_string());
    assert_eq!(s.reveal(), "hunter2");
    s.clear();
    assert!(s.is_empty(), "after clear() the Secret must report empty");
    assert_eq!(s.reveal(), "");
    assert_eq!(s.reveal_bytes(), b"");
}

#[test]
fn push_then_pop_then_clear_round_trip() {
    let mut s = Secret::new();
    s.push_char('a');
    s.push_char('b');
    s.push_char('c');
    assert_eq!(s.reveal(), "abc");
    s.pop_char();
    assert_eq!(s.reveal(), "ab");
    s.clear();
    assert!(s.is_empty());
}

#[test]
fn zeroize_writes_zero_bytes_to_owned_buffer() {
    use zeroize::Zeroize;
    let mut owned: Vec<u8> = b"hunter2".to_vec();
    let ptr = owned.as_ptr();
    let len = owned.len();
    assert_eq!(unsafe { std::slice::from_raw_parts(ptr, len) }, b"hunter2");
    owned.zeroize();
    assert!(
        owned.iter().all(|b| *b == 0),
        "zeroize must write zero bytes in place; got {:?}",
        owned
    );
}

#[tokio::test]
async fn ipc_frame_reveals_password_on_wire_but_request_debug_masks() {
    let req = DaemonRequest::UpdateServerConfig {
        base_url: "https://example.com".into(),
        username: "u".into(),
        password: Secret::from_string("hunter2".to_string()),
    };
    let dbg = format!("{:?}", req);
    assert!(
        !dbg.contains("hunter2"),
        "Debug of DaemonRequest must not contain plaintext password: {}",
        dbg
    );
    assert!(
        dbg.contains("***"),
        "Debug of DaemonRequest must show mask: {}",
        dbg
    );

    let frame = Frame::Request { id: 1, req };
    let mut buf: Vec<u8> = Vec::new();
    write_frame(&mut buf, &frame).await.unwrap();
    let body_str = String::from_utf8_lossy(&buf);
    assert!(
        body_str.contains("hunter2"),
        "wire frame must carry plaintext password, got: {}",
        body_str
    );

    let mut reader = buf.as_slice();
    let decoded = read_frame(&mut reader).await.unwrap();
    match decoded {
        Frame::Request {
            req: DaemonRequest::UpdateServerConfig { password, .. },
            ..
        } => assert_eq!(password.reveal(), "hunter2"),
        other => panic!("expected UpdateServerConfig, got {:?}", other),
    }
}

#[tokio::test]
async fn ipc_frame_test_server_connection_round_trips_plaintext() {
    let req = DaemonRequest::TestServerConnection {
        base_url: "https://example.com".into(),
        username: "u".into(),
        password: Secret::from_string("hunter2".to_string()),
    };
    let frame = Frame::Request { id: 2, req };
    let mut buf: Vec<u8> = Vec::new();
    write_frame(&mut buf, &frame).await.unwrap();
    let mut reader = buf.as_slice();
    let decoded = read_frame(&mut reader).await.unwrap();
    match decoded {
        Frame::Request {
            req: DaemonRequest::TestServerConnection { password, .. },
            ..
        } => assert_eq!(password.reveal(), "hunter2"),
        other => panic!("expected TestServerConnection, got {:?}", other),
    }
}

#[test]
fn config_debug_masks_resolved_password() {
    let mut c = ferrosonic::config::Config::default();
    c.password = Secret::from_string("hunter2".to_string());
    let d = format!("{:?}", c);
    assert!(
        !d.contains("hunter2"),
        "Config Debug must not contain plaintext: {}",
        d
    );
    assert!(d.contains("***"), "Config Debug must mask: {}", d);
}

#[test]
fn server_state_debug_masks_password() {
    let mut s = ferrosonic::app::state::ServerState::default();
    s.password = Secret::from_string("hunter2".to_string());
    let d = format!("{:?}", s);
    assert!(
        !d.contains("hunter2"),
        "ServerState Debug must not contain plaintext: {}",
        d
    );
    assert!(d.contains("***"), "ServerState Debug must mask: {}", d);
}

#[test]
fn clone_does_not_share_or_corrupt_storage_on_drop() {
    let original = Secret::from_string("hunter2".to_string());
    let cloned = original.clone();
    drop(cloned);
    assert_eq!(
        original.reveal(),
        "hunter2",
        "Dropping a clone must not zeroize the original",
    );
}
