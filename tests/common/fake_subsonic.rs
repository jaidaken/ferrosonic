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

    pub async fn expect_starred(&self) {
        Mock::given(method("GET"))
            .and(path("/rest/getStarred2"))
            .respond_with(ok_body(json!({
                "starred2": { "song": [] }
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
