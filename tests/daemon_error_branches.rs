//! Daemon core error-path coverage: inject Subsonic + mpv failures.

mod common;

use common::{song, TestDaemon};
use ferrosonic::daemon::core::PlayMode;
use serial_test::serial;

#[tokio::test]
#[serial]
async fn refresh_starred_handles_api_error_silently() {
    let td = TestDaemon::new().await;
    td.fake_subsonic
        .expect_error("getStarred2", 40, "auth")
        .await;
    td.core.refresh_starred().await;
    let s = td.state.read().await;
    assert!(s.library.starred_songs.is_empty());
}

#[tokio::test]
#[serial]
async fn refresh_random_handles_http_500() {
    let td = TestDaemon::new().await;
    td.fake_subsonic
        .expect_http_status("getRandomSongs", 500)
        .await;
    td.core.refresh_random().await;
    let s = td.state.read().await;
    assert!(s.library.random_songs.is_empty());
}

#[tokio::test]
#[serial]
async fn refresh_artists_handles_malformed_response() {
    let td = TestDaemon::new().await;
    td.fake_subsonic.expect_http_status("getArtists", 200).await;
    td.core.refresh_artists().await;
    let s = td.state.read().await;
    assert!(s.library.artists.is_empty());
}

#[tokio::test]
#[serial]
async fn refresh_playlists_handles_error() {
    let td = TestDaemon::new().await;
    td.fake_subsonic
        .expect_error("getPlaylists", 50, "err")
        .await;
    td.core.refresh_playlists().await;
    let s = td.state.read().await;
    assert!(s.library.playlists.is_empty());
}

#[tokio::test]
#[serial]
async fn toggle_star_returns_error_when_no_subsonic_client() {
    let td = TestDaemon::new().await;
    {
        let mut sub = td.core.subsonic.write().await;
        *sub = None;
    }
    {
        let mut s = td.state.write().await;
        s.queue.push(song("x", "X"));
    }
    let r = td.core.toggle_star_song("x").await;
    assert!(r.is_err());
}

#[tokio::test]
#[serial]
async fn toggle_star_propagates_subsonic_error() {
    let td = TestDaemon::new().await;
    td.fake_subsonic.expect_error("star", 40, "denied").await;
    {
        let mut s = td.state.write().await;
        s.queue.push(song("x", "X"));
    }
    let r = td.core.toggle_star_song("x").await;
    assert!(r.is_err());
}

#[tokio::test]
#[serial]
async fn load_album_songs_returns_empty_on_error() {
    let td = TestDaemon::new().await;
    td.fake_subsonic
        .expect_error("getAlbum", 70, "missing")
        .await;
    let songs = td.core.load_album_songs("alb-bad").await;
    assert!(songs.is_empty());
}

#[tokio::test]
#[serial]
async fn load_playlist_songs_returns_empty_on_error() {
    let td = TestDaemon::new().await;
    td.fake_subsonic
        .expect_error("getPlaylist", 70, "missing")
        .await;
    let songs = td.core.load_playlist_songs("p-bad").await;
    assert!(songs.is_empty());
}

#[tokio::test]
#[serial]
async fn load_artist_handles_error_gracefully() {
    let td = TestDaemon::new().await;
    td.fake_subsonic
        .expect_error("getArtist", 70, "missing")
        .await;
    td.core.load_artist("a-bad").await;
    let s = td.state.read().await;
    assert!(
        !s.library.albums_cache.contains_key("a-bad"),
        "failed load must not populate albums_cache"
    );
    assert!(s.library.albums_cache.is_empty());
}

#[tokio::test]
#[serial]
async fn search_with_subsonic_error_returns_empty_result() {
    let td = TestDaemon::new().await;
    td.fake_subsonic.expect_error("search3", 50, "boom").await;
    let r = td.core.search("anything", 5, 5, 5).await;
    assert!(r.artist.is_empty() && r.album.is_empty() && r.song.is_empty());
}

#[tokio::test]
#[serial]
async fn shuffle_library_with_empty_response_is_safe() {
    let td = TestDaemon::new().await;
    td.fake_subsonic.expect_random_songs(&[]).await;
    td.core.shuffle_library().await.unwrap();
    let s = td.state.read().await;
    assert!(s.queue.is_empty());
}

#[tokio::test]
#[serial]
async fn shuffle_library_with_subsonic_error_does_not_clear_queue() {
    let td = TestDaemon::new().await;
    td.fake_subsonic
        .expect_error("getRandomSongs", 50, "err")
        .await;
    {
        let mut s = td.state.write().await;
        s.queue.push(song("existing", "Keep"));
    }
    td.core.shuffle_library().await.unwrap();
    let s = td.state.read().await;
    assert_eq!(
        s.queue.len(),
        1,
        "shuffle error must not destroy existing queue"
    );
}

#[tokio::test]
#[serial]
async fn play_queue_position_out_of_bounds_is_silent_noop() {
    let td = TestDaemon::new().await;
    td.core
        .play_queue_position(99, PlayMode::Direct)
        .await
        .unwrap();
}

#[tokio::test]
#[serial]
async fn play_queue_position_with_no_subsonic_client_is_safe() {
    let td = TestDaemon::new().await;
    {
        let mut sub = td.core.subsonic.write().await;
        *sub = None;
    }
    {
        let mut s = td.state.write().await;
        s.queue.push(song("x", "X"));
    }
    td.core
        .play_queue_position(0, PlayMode::Direct)
        .await
        .unwrap();
}

#[tokio::test]
#[serial]
async fn update_server_config_invalid_url_returns_error() {
    let td = TestDaemon::new().await;
    let r = td
        .core
        .update_server_config("not a url", "u", &"p".into())
        .await;
    assert!(r.is_err());
}

#[tokio::test]
#[serial]
async fn next_track_on_empty_queue_is_silent_noop() {
    let td = TestDaemon::new().await;
    td.core.next_track().await.unwrap();
}

#[tokio::test]
#[serial]
async fn prev_track_on_empty_queue_is_silent_noop() {
    let td = TestDaemon::new().await;
    td.core.prev_track().await.unwrap();
}

#[tokio::test]
#[serial]
async fn auto_continue_with_subsonic_error_stops_cleanly() {
    use ferrosonic::daemon::state::PlaybackState;
    let td = TestDaemon::new().await;
    td.fake_subsonic
        .expect_error("getRandomSongs", 50, "err")
        .await;
    {
        let mut s = td.state.write().await;
        s.queue.push(song("last", "Last"));
        s.queue_position = Some(0);
        s.config.auto_continue = true;
        s.now_playing.state = PlaybackState::Playing;
    }
    td.core.next_track().await.unwrap();
    let s = td.state.read().await;
    assert_eq!(s.now_playing.state, PlaybackState::Stopped);
}

#[tokio::test]
#[serial]
async fn preload_next_track_with_subsonic_error_is_safe() {
    let td = TestDaemon::new().await;
    td.fake_subsonic
        .expect_error("getRandomSongs", 50, "err")
        .await;
    {
        let mut s = td.state.write().await;
        s.queue = vec![song("a", "A"), song("b", "B")];
        s.queue_position = Some(0);
    }
    td.core.preload_next_track(0).await;
    let s = td.state.read().await;
    assert_eq!(s.queue.len(), 2, "preload must not mutate queue");
    assert_eq!(s.queue[0].id, "a");
    assert_eq!(s.queue[1].id, "b");
    assert_eq!(s.queue_position, Some(0));
}

#[tokio::test]
#[serial]
async fn test_server_connection_with_unreachable_host_returns_failure() {
    let td = TestDaemon::new().await;
    let (ok, msg) = td
        .core
        .test_server_connection("http://127.0.0.1:1", "u", &"p".into())
        .await;
    assert!(!ok);
    assert!(msg.contains("failed") || msg.contains("Connection"));
}

#[tokio::test]
#[serial]
async fn clear_queue_history_with_position_zero_is_noop() {
    let td = TestDaemon::new().await;
    {
        let mut s = td.state.write().await;
        s.queue = vec![song("a", "A"), song("b", "B")];
        s.queue_position = Some(0);
    }
    let removed = td.core.clear_queue_history().await;
    assert_eq!(removed, 0);
}

#[tokio::test]
#[serial]
async fn move_queue_item_with_invalid_indices_is_noop() {
    let td = TestDaemon::new().await;
    {
        let mut s = td.state.write().await;
        s.queue = vec![song("a", "A"), song("b", "B")];
    }
    td.core.move_queue_item(99, 0).await;
    td.core.move_queue_item(0, 99).await;
    let s = td.state.read().await;
    assert_eq!(s.queue.len(), 2, "OOB move must preserve queue length");
    assert_eq!(s.queue[0].id, "a", "OOB move must preserve order");
    assert_eq!(s.queue[1].id, "b", "OOB move must preserve order");
}

#[tokio::test]
#[serial]
async fn shuffle_queue_with_empty_queue_is_noop() {
    let td = TestDaemon::new().await;
    td.core.shuffle_queue().await;
    let s = td.state.read().await;
    assert!(s.queue.is_empty(), "empty-queue shuffle must stay empty");
    assert_eq!(s.queue_position, None);
}

#[tokio::test]
#[serial]
async fn pause_playback_when_not_playing_is_noop() {
    let td = TestDaemon::new().await;
    td.core.pause_playback().await.unwrap();
}

#[tokio::test]
#[serial]
async fn resume_playback_when_not_paused_is_noop() {
    use ferrosonic::daemon::state::PlaybackState;
    let td = TestDaemon::new().await;
    {
        let mut s = td.state.write().await;
        s.now_playing.state = PlaybackState::Playing;
    }
    td.core.resume_playback().await.unwrap();
}
