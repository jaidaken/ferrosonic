//! FetchCoverArt id validation in InProcessClient::request. The id flows into a
//! Subsonic query; the guard rejects path/control chars and over-long ids before
//! fetching. Serving art for the rejected ids is the discriminator: a working
//! guard returns empty (never fetches), a broken guard fetches and returns bytes.

mod common;

use common::TestDaemon;
use ferrosonic::ipc::client::{DaemonClient, InProcessClient};
use ferrosonic::ipc::protocol::{DaemonRequest, DaemonResponse};
use serial_test::serial;

async fn fetch(client: &InProcessClient, id: &str) -> Vec<u8> {
    match client
        .request(DaemonRequest::FetchCoverArt {
            id: id.to_string(),
            size: 100,
        })
        .await
        .expect("request must not error")
    {
        DaemonResponse::CoverArt(bytes) => bytes,
        other => panic!("expected CoverArt, got {other:?}"),
    }
}

#[tokio::test]
#[serial]
async fn a_valid_id_returns_the_fetched_cover_art_bytes() {
    let td = TestDaemon::new().await;
    td.fake_subsonic
        .expect_get_cover_art("art1", vec![1, 2, 3, 4])
        .await;
    let client = InProcessClient::new(td.core.clone());

    assert_eq!(fetch(&client, "art1").await, vec![1, 2, 3, 4]);
}

#[tokio::test]
#[serial]
async fn an_id_containing_a_slash_is_rejected_before_fetch() {
    // Art is served for "a/b": if the char guard's `||`->`&&` mutation let it
    // through, the fetch would return these bytes. A working guard returns empty.
    let td = TestDaemon::new().await;
    td.fake_subsonic
        .expect_get_cover_art("a/b", vec![9, 9, 9])
        .await;
    let client = InProcessClient::new(td.core.clone());

    assert!(
        fetch(&client, "a/b").await.is_empty(),
        "an id with a path separator must be rejected, not fetched"
    );
}

#[tokio::test]
#[serial]
async fn an_id_at_the_max_length_is_accepted() {
    // 256 chars is within the limit; `>`->`>=` would reject it at exactly the cap.
    let td = TestDaemon::new().await;
    let id = "a".repeat(256);
    td.fake_subsonic.expect_get_cover_art(&id, vec![7, 7]).await;
    let client = InProcessClient::new(td.core.clone());

    assert_eq!(fetch(&client, &id).await, vec![7, 7]);
}

#[tokio::test]
#[serial]
async fn an_over_long_id_is_rejected_before_fetch() {
    // 257 chars exceeds the limit; `>`->`<`/`==` would stop rejecting it.
    let td = TestDaemon::new().await;
    let id = "a".repeat(257);
    td.fake_subsonic.expect_get_cover_art(&id, vec![5, 5]).await;
    let client = InProcessClient::new(td.core.clone());

    assert!(
        fetch(&client, &id).await.is_empty(),
        "an over-long id must be rejected, not fetched"
    );
}
