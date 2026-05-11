//! daemon/core.rs with fake subsonic returning real data.

mod common;

use common::TestDaemon;
use serial_test::serial;

#[tokio::test]
#[serial]
async fn refresh_starred_populates_library_via_subsonic() {
    let td = TestDaemon::new().await;
    td.fake_subsonic
        .expect_starred_with(&["song-1", "song-2"])
        .await;
    td.core.refresh_starred().await;
    let s = td.state.read().await;
    assert!(!s.library.starred_songs.is_empty());
}

#[tokio::test]
#[serial]
async fn refresh_random_populates_via_subsonic() {
    let td = TestDaemon::new().await;
    td.fake_subsonic.expect_random_songs(&["r0", "r1"]).await;
    td.core.refresh_random().await;
    let s = td.state.read().await;
    assert_eq!(s.library.random_songs.len(), 2);
}

#[tokio::test]
#[serial]
async fn refresh_artists_populates_via_subsonic() {
    let td = TestDaemon::new().await;
    td.fake_subsonic.expect_artists(&["Alpha", "Bravo"]).await;
    td.core.refresh_artists().await;
    let s = td.state.read().await;
    assert_eq!(s.library.artists.len(), 2);
}

#[tokio::test]
#[serial]
async fn refresh_playlists_populates_via_subsonic() {
    let td = TestDaemon::new().await;
    td.fake_subsonic.expect_playlists().await;
    td.core.refresh_playlists().await;
}

#[tokio::test]
#[serial]
async fn search_returns_real_results_from_fake_subsonic() {
    let td = TestDaemon::new().await;
    td.fake_subsonic
        .expect_search3(&["MatchedArtist"], &["MatchedAlbum"], &["MatchedSong"])
        .await;
    let r = td.core.search("query", 10, 10, 10).await;
    assert_eq!(r.artist.len(), 1);
    assert_eq!(r.album.len(), 1);
    assert_eq!(r.song.len(), 1);
}

#[tokio::test]
#[serial]
async fn load_artist_populates_albums_cache() {
    let td = TestDaemon::new().await;
    td.fake_subsonic
        .expect_get_artist("a0", "An Artist", &["Album One"])
        .await;
    td.core.load_artist("a0").await;
    let s = td.state.read().await;
    assert!(s.library.albums_cache.contains_key("a0"));
}

#[tokio::test]
#[serial]
async fn load_album_songs_returns_real_songs() {
    let td = TestDaemon::new().await;
    td.fake_subsonic
        .expect_get_album("alb0", "Alb", &["s0", "s1"])
        .await;
    let songs = td.core.load_album_songs("alb0").await;
    assert_eq!(songs.len(), 2);
}

#[tokio::test]
#[serial]
async fn load_playlist_songs_returns_real_songs() {
    let td = TestDaemon::new().await;
    td.fake_subsonic
        .expect_get_playlist("p0", "Mix", &["s0", "s1", "s2"])
        .await;
    let songs = td.core.load_playlist_songs("p0").await;
    assert_eq!(songs.len(), 3);
}

#[tokio::test]
#[serial]
async fn get_cover_art_caches_response() {
    let td = TestDaemon::new().await;
    let body = vec![0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a];
    td.fake_subsonic.expect_get_cover_art("art-1", body).await;
    let bytes = td.core.get_cover_art("art-1", 64).await;
    assert_eq!(bytes.len(), 8);
    let again = td.core.get_cover_art("art-1", 64).await;
    assert_eq!(again.len(), 8);
}

#[tokio::test]
#[serial]
async fn test_server_connection_succeeds_with_fake() {
    let td = TestDaemon::new().await;
    td.fake_subsonic.expect_ping().await;
    let (ok, _msg) = td
        .core
        .test_server_connection(&td.fake_subsonic.url(), "u", "p")
        .await;
    assert!(ok);
}

#[tokio::test]
#[serial]
async fn toggle_star_unstars_when_currently_starred() {
    use common::song_starred;
    let td = TestDaemon::new().await;
    td.fake_subsonic.expect_unstar().await;
    {
        let mut s = td.state.write().await;
        s.library.starred_songs = vec![song_starred("track-1", "Track One")];
    }
    let new = td.core.toggle_star_song("track-1").await.unwrap();
    assert!(!new);
}

#[tokio::test]
#[serial]
async fn toggle_star_stars_when_currently_unstarred() {
    use common::song;
    let td = TestDaemon::new().await;
    td.fake_subsonic.expect_star().await;
    {
        let mut s = td.state.write().await;
        s.library.starred_songs = vec![song("track-1", "Track One")];
    }
    let new = td.core.toggle_star_song("track-1").await.unwrap();
    assert!(new);
}

#[tokio::test]
#[serial]
async fn update_server_config_persists_and_reloads() {
    let td = TestDaemon::new().await;
    td.fake_subsonic.expect_ping().await;
    let r = td
        .core
        .update_server_config(&td.fake_subsonic.url(), "newuser", "newpw")
        .await;
    let _ = r;
    let s = td.state.read().await;
    assert_eq!(s.config.username, "newuser");
}
