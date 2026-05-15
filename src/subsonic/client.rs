//! Subsonic API client

use std::sync::Arc;

use reqwest::Client;
use tracing::{debug, info};
use url::Url;

use super::auth::generate_auth_params;
use super::models::*;
use crate::error::SubsonicError;
use crate::secret::Secret;

const CLIENT_NAME: &str = "ferrosonic-rs";
const API_VERSION: &str = "1.16.1";

#[derive(Clone)]
pub struct SubsonicClient {
    base_url: Url,
    username: String,
    /// Arc shares the secret across clones without duplicating the heap bytes.
    password: Arc<Secret>,
    http: Client,
}

impl SubsonicClient {
    pub fn new(base_url: &str, username: &str, password: &Secret) -> Result<Self, SubsonicError> {
        let base_url = Url::parse(base_url)?;

        let http = Client::builder()
            .user_agent(CLIENT_NAME)
            .build()
            .map_err(SubsonicError::Http)?;

        Ok(Self {
            base_url,
            username: username.to_string(),
            password: Arc::new(password.clone()),
            http,
        })
    }

    #[doc(hidden)]
    pub fn base_url(&self) -> &str {
        self.base_url.as_str()
    }

    fn build_url(&self, endpoint: &str) -> Result<Url, SubsonicError> {
        let mut url = self.base_url.join(&format!("rest/{}", endpoint))?;

        let (salt, token) = generate_auth_params(&self.password);

        url.query_pairs_mut()
            .append_pair("u", &self.username)
            .append_pair("t", &token)
            .append_pair("s", &salt)
            .append_pair("v", API_VERSION)
            .append_pair("c", CLIENT_NAME)
            .append_pair("f", "json");

        Ok(url)
    }

    async fn request_action(&self, endpoint: &str) -> Result<(), SubsonicError> {
        let url = self.build_url(endpoint)?;
        let response = self.http.get(url).send().await?;
        let text = response.text().await?;
        let parsed: SubsonicResponse<serde_json::Value> = serde_json::from_str(&text)
            .map_err(|e| SubsonicError::Parse(format!("Failed to parse response: {}", e)))?;
        let inner = parsed.subsonic_response;
        if inner.status != "ok" {
            let (code, message) = match inner.error {
                Some(e) => (e.code, e.message),
                None => (0, "Unknown error".to_string()),
            };
            return Err(SubsonicError::Api { code, message });
        }
        Ok(())
    }

    pub async fn search3(
        &self,
        query: &str,
        artist_count: u32,
        album_count: u32,
        song_count: u32,
    ) -> Result<SearchResult3, SubsonicError> {
        let endpoint = format!(
            "search3?query={}&artistCount={}&albumCount={}&songCount={}",
            urlencoding::encode(query),
            artist_count,
            album_count,
            song_count,
        );
        let data: Search3Data = self.request(&endpoint).await?;
        Ok(data.result)
    }

    pub async fn star_song(&self, id: &str) -> Result<(), SubsonicError> {
        self.request_action(&format!("star?id={}", urlencoding::encode(id)))
            .await
    }

    pub async fn unstar_song(&self, id: &str) -> Result<(), SubsonicError> {
        self.request_action(&format!("unstar?id={}", urlencoding::encode(id)))
            .await
    }

    async fn request<T>(&self, endpoint: &str) -> Result<T, SubsonicError>
    where
        T: serde::de::DeserializeOwned,
    {
        let url = self.build_url(endpoint)?;
        debug!(
            "Requesting: {}",
            url.as_str().split('?').next().unwrap_or("")
        );

        let response = self.http.get(url).send().await?;
        let text = response.text().await?;

        let parsed: SubsonicResponse<T> = serde_json::from_str(&text)
            .map_err(|e| SubsonicError::Parse(format!("Failed to parse response: {}", e)))?;

        let inner = parsed.subsonic_response;

        if inner.status != "ok" {
            if let Some(error) = inner.error {
                return Err(SubsonicError::Api {
                    code: error.code,
                    message: error.message,
                });
            }
            return Err(SubsonicError::Api {
                code: 0,
                message: "Unknown error".to_string(),
            });
        }

        inner
            .data
            .ok_or_else(|| SubsonicError::Parse("Empty response data".to_string()))
    }

    pub async fn ping(&self) -> Result<(), SubsonicError> {
        let url = self.build_url("ping")?;
        debug!("Pinging server");

        let response = self.http.get(url).send().await?;
        let text = response.text().await?;

        let parsed: SubsonicResponse<PingData> = serde_json::from_str(&text)
            .map_err(|e| SubsonicError::Parse(format!("Failed to parse ping response: {}", e)))?;

        if parsed.subsonic_response.status != "ok" {
            if let Some(error) = parsed.subsonic_response.error {
                return Err(SubsonicError::Api {
                    code: error.code,
                    message: error.message,
                });
            }
        }

        info!("Server ping successful");
        Ok(())
    }

    pub async fn get_starred_songs(&self) -> Result<Vec<Child>, SubsonicError> {
        let data: StarredSongsData = self.request("getStarred2").await?;
        let songs = data.starred_songs.song;

        debug!("Fetched {} songs", songs.len());
        Ok(songs)
    }

    pub async fn get_random_songs(&self) -> Result<Vec<Child>, SubsonicError> {
        let data: RandomSongsData = self.request("getRandomSongs?size=500").await?;
        let songs = data.random_songs.song;

        debug!("Fetched {} songs", songs.len());
        Ok(songs)
    }

    pub async fn get_artists(&self) -> Result<Vec<Artist>, SubsonicError> {
        let data: ArtistsData = self.request("getArtists").await?;

        let artists: Vec<Artist> = data
            .artists
            .index
            .into_iter()
            .flat_map(|idx| idx.artist)
            .collect();

        debug!("Fetched {} artists", artists.len());
        Ok(artists)
    }

    pub async fn get_artist(&self, id: &str) -> Result<(Artist, Vec<Album>), SubsonicError> {
        let url = self.build_url(&format!("getArtist?id={}", id))?;
        debug!("Fetching artist: {}", id);

        let response = self.http.get(url).send().await?;
        let text = response.text().await?;

        let parsed: SubsonicResponse<ArtistData> = serde_json::from_str(&text)
            .map_err(|e| SubsonicError::Parse(format!("Failed to parse artist response: {}", e)))?;

        if parsed.subsonic_response.status != "ok" {
            if let Some(error) = parsed.subsonic_response.error {
                return Err(SubsonicError::Api {
                    code: error.code,
                    message: error.message,
                });
            }
        }

        let detail = parsed
            .subsonic_response
            .data
            .ok_or_else(|| SubsonicError::Parse("Empty artist data".to_string()))?
            .artist;

        let artist = Artist {
            id: detail.id,
            name: detail.name.clone(),
            album_count: Some(detail.album.len() as i32),
            cover_art: None,
        };

        debug!(
            "Fetched artist {} with {} albums",
            detail.name,
            detail.album.len()
        );
        Ok((artist, detail.album))
    }

    pub async fn get_album(&self, id: &str) -> Result<(Album, Vec<Child>), SubsonicError> {
        let url = self.build_url(&format!("getAlbum?id={}", id))?;
        debug!("Fetching album: {}", id);

        let response = self.http.get(url).send().await?;
        let text = response.text().await?;

        let parsed: SubsonicResponse<AlbumData> = serde_json::from_str(&text)
            .map_err(|e| SubsonicError::Parse(format!("Failed to parse album response: {}", e)))?;

        if parsed.subsonic_response.status != "ok" {
            if let Some(error) = parsed.subsonic_response.error {
                return Err(SubsonicError::Api {
                    code: error.code,
                    message: error.message,
                });
            }
        }

        let detail = parsed
            .subsonic_response
            .data
            .ok_or_else(|| SubsonicError::Parse("Empty album data".to_string()))?
            .album;

        let album = Album {
            id: detail.id,
            name: detail.name.clone(),
            artist: detail.artist,
            artist_id: detail.artist_id,
            cover_art: None,
            song_count: Some(detail.song.len() as i32),
            duration: None,
            year: detail.year,
            genre: None,
        };

        debug!(
            "Fetched album {} with {} songs",
            detail.name,
            detail.song.len()
        );
        Ok((album, detail.song))
    }

    pub async fn get_playlists(&self) -> Result<Vec<Playlist>, SubsonicError> {
        let data: PlaylistsData = self.request("getPlaylists").await?;
        let playlists = data.playlists.playlist;
        debug!("Fetched {} playlists", playlists.len());
        Ok(playlists)
    }

    pub async fn get_playlist(&self, id: &str) -> Result<(Playlist, Vec<Child>), SubsonicError> {
        let url = self.build_url(&format!("getPlaylist?id={}", id))?;
        debug!("Fetching playlist: {}", id);

        let response = self.http.get(url).send().await?;
        let text = response.text().await?;

        let parsed: SubsonicResponse<PlaylistData> = serde_json::from_str(&text).map_err(|e| {
            SubsonicError::Parse(format!("Failed to parse playlist response: {}", e))
        })?;

        if parsed.subsonic_response.status != "ok" {
            if let Some(error) = parsed.subsonic_response.error {
                return Err(SubsonicError::Api {
                    code: error.code,
                    message: error.message,
                });
            }
        }

        let detail = parsed
            .subsonic_response
            .data
            .ok_or_else(|| SubsonicError::Parse("Empty playlist data".to_string()))?
            .playlist;

        let playlist = Playlist {
            id: detail.id,
            name: detail.name.clone(),
            owner: detail.owner,
            song_count: detail.song_count,
            duration: detail.duration,
            cover_art: None,
            public: None,
            comment: None,
        };

        debug!(
            "Fetched playlist {} with {} songs",
            detail.name,
            detail.entry.len()
        );
        Ok((playlist, detail.entry))
    }

    /// `size` is the longest-edge in pixels.
    pub async fn get_cover_art(&self, id: &str, size: u32) -> Result<Vec<u8>, SubsonicError> {
        let mut url = self.base_url.join("rest/getCoverArt")?;
        let (salt, token) = generate_auth_params(&self.password);
        url.query_pairs_mut()
            .append_pair("id", id)
            .append_pair("size", &size.to_string())
            .append_pair("u", &self.username)
            .append_pair("t", &token)
            .append_pair("s", &salt)
            .append_pair("v", API_VERSION)
            .append_pair("c", CLIENT_NAME);

        let resp = self.http.get(url).send().await?;
        if !resp.status().is_success() {
            return Err(SubsonicError::Api {
                code: resp.status().as_u16() as i32,
                message: format!("getCoverArt HTTP {}", resp.status()),
            });
        }
        let bytes = resp.bytes().await?;
        Ok(bytes.to_vec())
    }

    /// Build the signed `rest/stream` URL for `song_id`. Adds Subsonic auth params (`u`, `t`, `s`, `v`, `c`) plus the song `id`. No network IO; this is pure URL assembly.
    ///
    /// ```
    /// use ferrosonic::secret::Secret;
    /// use ferrosonic::subsonic::client::SubsonicClient;
    /// let pw = Secret::from_string("pw".to_string());
    /// let c = SubsonicClient::new("https://example.com/", "alice", &pw).unwrap();
    /// let url = c.get_stream_url("song-42").unwrap();
    /// assert!(url.starts_with("https://example.com/rest/stream?"));
    /// assert!(url.contains("id=song-42"));
    /// assert!(url.contains("u=alice"));
    /// assert!(url.contains("&t="));
    /// assert!(url.contains("&s="));
    /// ```
    pub fn get_stream_url(&self, song_id: &str) -> Result<String, SubsonicError> {
        let mut url = self.base_url.join("rest/stream")?;

        let (salt, token) = generate_auth_params(&self.password);

        url.query_pairs_mut()
            .append_pair("id", song_id)
            .append_pair("u", &self.username)
            .append_pair("t", &token)
            .append_pair("s", &salt)
            .append_pair("v", API_VERSION)
            .append_pair("c", CLIENT_NAME);

        Ok(url.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    impl SubsonicClient {
        /// Parse song ID from a stream URL
        fn parse_song_id_from_url(url: &str) -> Option<String> {
            let parsed = Url::parse(url).ok()?;
            parsed
                .query_pairs()
                .find(|(k, _)| k == "id")
                .map(|(_, v)| v.to_string())
        }
    }

    #[test]
    fn test_parse_song_id() {
        let url = "https://example.com/rest/stream?id=12345&u=user&t=token&s=salt&v=1.16.1&c=test";
        let id = SubsonicClient::parse_song_id_from_url(url);
        assert_eq!(id, Some("12345".to_string()));
    }

    #[test]
    fn test_parse_song_id_missing() {
        let url = "https://example.com/rest/stream?u=user";
        let id = SubsonicClient::parse_song_id_from_url(url);
        assert_eq!(id, None);
    }
}
