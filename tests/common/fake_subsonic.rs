//! Wiremock wrapper for the Subsonic REST API.

use serde_json::{json, Value};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

pub struct FakeSubsonic {
    server: MockServer,
}

impl FakeSubsonic {
    pub async fn start() -> Self {
        Self {
            server: MockServer::start().await,
        }
    }

    pub fn url(&self) -> String {
        self.server.uri()
    }

    pub async fn expect_ping(&self) {
        Mock::given(method("GET"))
            .and(path("/rest/ping"))
            .respond_with(ok_body(json!({})))
            .mount(&self.server)
            .await;
    }

    pub async fn expect_artists(&self, artists: &[&str]) {
        let indexes: Vec<Value> = artists
            .iter()
            .enumerate()
            .map(|(i, name)| {
                let letter = name
                    .chars()
                    .next()
                    .map(|c| c.to_ascii_uppercase().to_string())
                    .unwrap_or_else(|| "?".to_string());
                json!({
                    "name": letter,
                    "artist": [{
                        "id": format!("artist-{}", i),
                        "name": name,
                        "albumCount": 1
                    }]
                })
            })
            .collect();
        Mock::given(method("GET"))
            .and(path("/rest/getArtists"))
            .respond_with(ok_body(json!({
                "artists": { "index": indexes }
            })))
            .mount(&self.server)
            .await;
    }

    pub async fn expect_random_songs(&self, songs: &[&str]) {
        let song_list: Vec<Value> = songs
            .iter()
            .enumerate()
            .map(|(i, title)| {
                json!({
                    "id": format!("song-{}", i),
                    "title": title,
                    "artist": "Test Artist",
                    "album": "Test Album",
                    "duration": 180,
                    "isDir": false
                })
            })
            .collect();
        Mock::given(method("GET"))
            .and(path("/rest/getRandomSongs"))
            .respond_with(ok_body(json!({
                "randomSongs": { "song": song_list }
            })))
            .mount(&self.server)
            .await;
    }

    /// Like `expect_random_songs` but each song carries the given rating
    /// (1-5), for playback-filter tests.
    pub async fn expect_random_songs_rated(&self, songs: &[(&str, u8)]) {
        let song_list: Vec<Value> = songs
            .iter()
            .enumerate()
            .map(|(i, (title, rating))| {
                json!({
                    "id": format!("song-{}", i),
                    "title": title,
                    "artist": "Test Artist",
                    "album": "Test Album",
                    "duration": 180,
                    "isDir": false,
                    "userRating": rating,
                })
            })
            .collect();
        Mock::given(method("GET"))
            .and(path("/rest/getRandomSongs"))
            .respond_with(ok_body(json!({
                "randomSongs": { "song": song_list }
            })))
            .mount(&self.server)
            .await;
    }

    pub async fn expect_starred(&self) {
        self.expect_starred_with(&[]).await;
    }

    pub async fn expect_starred_with(&self, songs: &[&str]) {
        let song_list: Vec<Value> = songs
            .iter()
            .enumerate()
            .map(|(i, title)| {
                json!({
                    "id": format!("starred-{}", i),
                    "title": title,
                    "starred": "2026-05-11T00:00:00Z",
                    "artist": "X",
                    "album": "Y"
                })
            })
            .collect();
        Mock::given(method("GET"))
            .and(path("/rest/getStarred2"))
            .respond_with(ok_body(json!({
                "starred2": { "song": song_list }
            })))
            .mount(&self.server)
            .await;
    }

    /// Like `expect_starred_with` but holds the response for `delay_ms`, so a
    /// test can act (e.g. bump config_gen) while the refresh is mid-request.
    pub async fn expect_starred_with_delay(&self, songs: &[&str], delay_ms: u64) {
        let song_list: Vec<Value> = songs
            .iter()
            .enumerate()
            .map(|(i, title)| {
                json!({
                    "id": format!("starred-{}", i),
                    "title": title,
                    "starred": "2026-05-11T00:00:00Z",
                    "artist": "X",
                    "album": "Y"
                })
            })
            .collect();
        Mock::given(method("GET"))
            .and(path("/rest/getStarred2"))
            .respond_with(
                ok_body(json!({ "starred2": { "song": song_list } }))
                    .set_delay(std::time::Duration::from_millis(delay_ms)),
            )
            .mount(&self.server)
            .await;
    }

    pub async fn expect_music_folders(&self, folders: &[(i64, &str)]) {
        let list: Vec<Value> = folders
            .iter()
            .map(|(id, name)| json!({ "id": id, "name": name }))
            .collect();
        Mock::given(method("GET"))
            .and(path("/rest/getMusicFolders"))
            .respond_with(ok_body(json!({
                "musicFolders": { "musicFolder": list }
            })))
            .mount(&self.server)
            .await;
    }

    pub async fn expect_playlists(&self) {
        Mock::given(method("GET"))
            .and(path("/rest/getPlaylists"))
            .respond_with(ok_body(json!({
                "playlists": { "playlist": [] }
            })))
            .mount(&self.server)
            .await;
    }

    pub async fn expect_open_subsonic_extensions(&self, names: &[&str]) {
        let exts: Vec<Value> = names
            .iter()
            .map(|n| json!({ "name": n, "versions": [1] }))
            .collect();
        Mock::given(method("GET"))
            .and(path("/rest/getOpenSubsonicExtensions"))
            .respond_with(ok_body(json!({ "openSubsonicExtensions": exts })))
            .mount(&self.server)
            .await;
    }

    pub async fn expect_structured_lyrics(&self, song_id: &str, sources: Value) {
        Mock::given(method("GET"))
            .and(path("/rest/getLyricsBySongId"))
            .and(wiremock::matchers::query_param("id", song_id))
            .respond_with(ok_body(json!({
                "lyricsList": { "structuredLyrics": sources }
            })))
            .mount(&self.server)
            .await;
    }

    pub async fn expect_classic_lyrics(&self, artist: &str, title: &str, value: &str) {
        Mock::given(method("GET"))
            .and(path("/rest/getLyrics"))
            .and(wiremock::matchers::query_param("artist", artist))
            .and(wiremock::matchers::query_param("title", title))
            .respond_with(ok_body(json!({
                "lyrics": { "artist": artist, "title": title, "value": value }
            })))
            .mount(&self.server)
            .await;
    }

    pub async fn expect_malformed(&self, endpoint: &str) {
        Mock::given(method("GET"))
            .and(path(format!("/rest/{endpoint}")))
            .respond_with(ResponseTemplate::new(200).set_body_string("not json"))
            .mount(&self.server)
            .await;
    }

    pub async fn expect_scrobble(&self) {
        Mock::given(method("GET"))
            .and(path("/rest/scrobble"))
            .respond_with(ok_body(json!({})))
            .mount(&self.server)
            .await;
    }

    pub async fn expect_report_playback(&self) {
        Mock::given(method("GET"))
            .and(path("/rest/reportPlayback"))
            .respond_with(ok_body(json!({})))
            .mount(&self.server)
            .await;
    }

    pub async fn expect_create_playlist(&self) {
        Mock::given(method("GET"))
            .and(path("/rest/createPlaylist"))
            .respond_with(ok_body(json!({})))
            .mount(&self.server)
            .await;
    }

    pub async fn expect_update_playlist(&self) {
        Mock::given(method("GET"))
            .and(path("/rest/updatePlaylist"))
            .respond_with(ok_body(json!({})))
            .mount(&self.server)
            .await;
    }

    pub async fn expect_delete_playlist(&self) {
        Mock::given(method("GET"))
            .and(path("/rest/deletePlaylist"))
            .respond_with(ok_body(json!({})))
            .mount(&self.server)
            .await;
    }

    pub async fn expect_star(&self) {
        Mock::given(method("GET"))
            .and(path("/rest/star"))
            .respond_with(ok_body(json!({})))
            .mount(&self.server)
            .await;
    }

    pub async fn expect_unstar(&self) {
        Mock::given(method("GET"))
            .and(path("/rest/unstar"))
            .respond_with(ok_body(json!({})))
            .mount(&self.server)
            .await;
    }

    pub async fn expect_rating_response(&self, rating: u8, status: u16, delay_ms: u64) {
        Mock::given(method("GET"))
            .and(path("/rest/setRating"))
            .and(wiremock::matchers::query_param(
                "rating",
                rating.to_string(),
            ))
            .respond_with(
                ResponseTemplate::new(status)
                    .set_body_json(if status == 200 {
                        json!({"subsonic-response": {"status": "ok", "version": "1.16.1"}})
                    } else {
                        json!({"subsonic-response": {"status": "failed", "version": "1.16.1",
                        "error": {"code": 0, "message": "rating rejected"}}})
                    })
                    .set_delay(std::time::Duration::from_millis(delay_ms)),
            )
            .mount(&self.server)
            .await;
    }

    pub async fn expect_set_rating(&self) {
        Mock::given(method("GET"))
            .and(path("/rest/setRating"))
            .respond_with(ok_body(json!({})))
            .mount(&self.server)
            .await;
    }

    pub async fn expect_search3(&self, artists: &[&str], albums: &[&str], songs: &[&str]) {
        let artist_list: Vec<Value> = artists
            .iter()
            .enumerate()
            .map(|(i, name)| json!({"id": format!("artist-{}", i), "name": name}))
            .collect();
        let album_list: Vec<Value> = albums
            .iter()
            .enumerate()
            .map(|(i, name)| json!({"id": format!("album-{}", i), "name": name}))
            .collect();
        let song_list: Vec<Value> = songs
            .iter()
            .enumerate()
            .map(|(i, title)| {
                json!({
                    "id": format!("song-{}", i),
                    "title": title,
                    "artist": "Test Artist",
                    "album": "Test Album"
                })
            })
            .collect();
        Mock::given(method("GET"))
            .and(path("/rest/search3"))
            .respond_with(ok_body(json!({
                "searchResult3": {
                    "artist": artist_list,
                    "album": album_list,
                    "song": song_list
                }
            })))
            .mount(&self.server)
            .await;
    }

    pub async fn expect_error(&self, endpoint: &str, code: i32, message: &str) {
        Mock::given(method("GET"))
            .and(path(format!("/rest/{}", endpoint)))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "subsonic-response": {
                    "status": "failed",
                    "version": "1.16.1",
                    "error": { "code": code, "message": message }
                }
            })))
            .mount(&self.server)
            .await;
    }

    /// A non-ok response that carries no `error` object, to exercise the
    /// fall-through path in the hand-rolled get_* handlers.
    pub async fn expect_failed_without_error(&self, endpoint: &str) {
        Mock::given(method("GET"))
            .and(path(format!("/rest/{}", endpoint)))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "subsonic-response": { "status": "failed", "version": "1.16.1" }
            })))
            .mount(&self.server)
            .await;
    }

    pub async fn expect_http_status(&self, endpoint: &str, status: u16) {
        Mock::given(method("GET"))
            .and(path(format!("/rest/{}", endpoint)))
            .respond_with(ResponseTemplate::new(status))
            .mount(&self.server)
            .await;
    }

    pub async fn expect_get_artist(&self, id: &str, name: &str, albums: &[&str]) {
        let album_list: Vec<Value> = albums
            .iter()
            .enumerate()
            .map(|(i, n)| json!({"id": format!("alb-{}", i), "name": n, "artist": name, "artistId": id}))
            .collect();
        Mock::given(method("GET"))
            .and(path("/rest/getArtist"))
            .and(wiremock::matchers::query_param("id", id))
            .respond_with(ok_body(json!({
                "artist": {"id": id, "name": name, "album": album_list}
            })))
            .mount(&self.server)
            .await;
    }

    pub async fn expect_get_album(&self, id: &str, name: &str, songs: &[&str]) {
        let song_list: Vec<Value> = songs
            .iter()
            .enumerate()
            .map(|(i, title)| {
                json!({
                    "id": format!("song-{}", i),
                    "title": title,
                    "artist": "Test Artist",
                    "album": name,
                    "duration": 180,
                    "isDir": false
                })
            })
            .collect();
        Mock::given(method("GET"))
            .and(path("/rest/getAlbum"))
            .and(wiremock::matchers::query_param("id", id))
            .respond_with(ok_body(json!({
                "album": {
                    "id": id,
                    "name": name,
                    "song": song_list
                }
            })))
            .mount(&self.server)
            .await;
    }

    /// Mocks `getAlbumList2?type=random` returning one album, plus `getAlbum`
    /// for that album's id returning `songs`.
    pub async fn expect_random_album(&self, id: &str, name: &str, songs: &[&str]) {
        Mock::given(method("GET"))
            .and(path("/rest/getAlbumList2"))
            .and(wiremock::matchers::query_param("type", "random"))
            .respond_with(ok_body(json!({
                "albumList2": { "album": [{"id": id, "name": name}] }
            })))
            .mount(&self.server)
            .await;
        self.expect_get_album(id, name, songs).await;
    }

    /// Mocks one `getAlbumList2` category plus `getAlbum` for its first result.
    pub async fn expect_quick_play_album(
        &self,
        sort_type: &str,
        id: &str,
        name: &str,
        songs: &[&str],
    ) {
        Mock::given(method("GET"))
            .and(path("/rest/getAlbumList2"))
            .and(wiremock::matchers::query_param("type", sort_type))
            .respond_with(ok_body(json!({
                "albumList2": { "album": [{"id": id, "name": name}] }
            })))
            .mount(&self.server)
            .await;
        self.expect_get_album(id, name, songs).await;
    }

    /// Mocks an empty `getAlbumList2` category.
    pub async fn expect_no_quick_play_album(&self, sort_type: &str) {
        self.expect_no_quick_play_album_with_delay(sort_type, 0)
            .await;
    }

    /// Mocks an empty category after a response delay.
    pub async fn expect_no_quick_play_album_with_delay(&self, sort_type: &str, delay_ms: u64) {
        Mock::given(method("GET"))
            .and(path("/rest/getAlbumList2"))
            .and(wiremock::matchers::query_param("type", sort_type))
            .respond_with(
                ok_body(json!({"albumList2": {"album": []}}))
                    .set_delay(std::time::Duration::from_millis(delay_ms)),
            )
            .mount(&self.server)
            .await;
    }

    /// Mocks `getAlbumList2?type=random` returning no albums (empty library).
    pub async fn expect_no_random_album(&self) {
        self.expect_no_random_album_with_delay(0).await;
    }

    pub async fn expect_no_random_album_with_delay(&self, delay_ms: u64) {
        Mock::given(method("GET"))
            .and(path("/rest/getAlbumList2"))
            .and(wiremock::matchers::query_param("type", "random"))
            .respond_with(
                ok_body(json!({
                    "albumList2": { "album": [] }
                }))
                .set_delay(std::time::Duration::from_millis(delay_ms)),
            )
            .mount(&self.server)
            .await;
    }

    pub async fn expect_get_playlist(&self, id: &str, name: &str, songs: &[&str]) {
        let song_list: Vec<Value> = songs
            .iter()
            .enumerate()
            .map(|(i, title)| {
                json!({
                    "id": format!("song-{}", i),
                    "title": title,
                    "artist": "X",
                    "album": "Y",
                    "duration": 180
                })
            })
            .collect();
        Mock::given(method("GET"))
            .and(path("/rest/getPlaylist"))
            .and(wiremock::matchers::query_param("id", id))
            .respond_with(ok_body(json!({
                "playlist": {
                    "id": id,
                    "name": name,
                    "entry": song_list
                }
            })))
            .mount(&self.server)
            .await;
    }

    pub async fn expect_get_cover_art(&self, id: &str, body: Vec<u8>) {
        Mock::given(method("GET"))
            .and(path("/rest/getCoverArt"))
            .and(wiremock::matchers::query_param("id", id))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_bytes(body)
                    .insert_header("content-type", "image/png"),
            )
            .mount(&self.server)
            .await;
    }

    pub async fn expect_get_playlists_with(&self, playlists: &[(&str, &str)]) {
        let pl_list: Vec<Value> = playlists
            .iter()
            .map(|(id, name)| {
                json!({
                    "id": id,
                    "name": name,
                    "songCount": 5,
                    "duration": 900
                })
            })
            .collect();
        Mock::given(method("GET"))
            .and(path("/rest/getPlaylists"))
            .respond_with(ok_body(json!({
                "playlists": { "playlist": pl_list }
            })))
            .mount(&self.server)
            .await;
    }

    pub async fn expect_stream_for(&self, song_id: &str, body: Vec<u8>) {
        Mock::given(method("GET"))
            .and(path("/rest/stream"))
            .and(wiremock::matchers::query_param("id", song_id))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_bytes(body)
                    .insert_header("content-type", "audio/mpeg"),
            )
            .mount(&self.server)
            .await;
    }

    pub async fn received_requests(&self) -> Vec<wiremock::Request> {
        self.server.received_requests().await.unwrap_or_default()
    }
}

fn ok_body(extra: Value) -> ResponseTemplate {
    let mut response = serde_json::Map::new();
    response.insert("status".into(), Value::String("ok".into()));
    response.insert("version".into(), Value::String("1.16.1".into()));
    if let Value::Object(obj) = extra {
        for (k, v) in obj {
            response.insert(k, v);
        }
    }
    ResponseTemplate::new(200).set_body_json(json!({
        "subsonic-response": Value::Object(response)
    }))
}
