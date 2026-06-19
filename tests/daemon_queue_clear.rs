//! The queue snapshot belongs to a daemon session: QueueSnapshot::remove
//! (called on daemon shutdown) deletes it so the next start is empty.

mod common;
use ferrosonic::daemon::persistence::QueueSnapshot;
use ferrosonic::subsonic::models::Child;
use serial_test::serial;

fn snapshot() -> QueueSnapshot {
    QueueSnapshot {
        queue: vec![Child {
            id: "s1".into(),
            title: "Track".into(),
            parent: None,
            is_dir: false,
            album: Some("Album".into()),
            artist: Some("Artist".into()),
            artist_id: None,
            album_id: None,
            track: None,
            year: None,
            genre: None,
            cover_art: None,
            size: None,
            content_type: None,
            suffix: None,
            duration: Some(180),
            bit_rate: None,
            path: None,
            disc_number: None,
            starred: None,
        }],
        position: Some(0),
    }
}

#[test]
#[serial]
fn remove_deletes_the_snapshot_so_the_next_start_is_empty() {
    let dir = common::tempdir();
    std::env::set_var("FERROSONIC_CONFIG_DIR", dir.path());

    snapshot().save().expect("save snapshot");
    assert!(
        QueueSnapshot::load().is_some(),
        "a saved snapshot is restored on the next start"
    );

    QueueSnapshot::remove();
    assert!(
        QueueSnapshot::load().is_none(),
        "after remove, the next start finds no queue to restore"
    );

    std::env::remove_var("FERROSONIC_CONFIG_DIR");
}
