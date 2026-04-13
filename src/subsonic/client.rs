//! Subsonic API client

use reqwest::Client;
use tracing::{debug, info};
use url::Url;

use super::auth::generate_auth_params;
use super::models::*;
use crate::error::SubsonicError;

/// Client name sent to Subsonic server
const CLIENT_NAME: &str = "ferrosonic-rs";
/// API version we support
const API_VERSION: &str = "1.16.1";

/// Subsonic API client
#[derive(Clone)]
pub struct SubsonicClient {
    /// Base URL of the Subsonic server
    base_url: Url,
    /// Username for authentication
    username: String,
    /// Password for authentication (stored for stream URLs)
    password: String,
    /// HTTP client
    http: Client,
}

impl SubsonicClient {
    /// Create a new Subsonic client
    pub fn new(base_url: &str, username: &str, password: &str) -> Result<Self, SubsonicError> {
        let base_url = Url::parse(base_url)?;

        let http = Client::builder()
            .user_agent(CLIENT_NAME)
            .build()
            .map_err(SubsonicError::Http)?;

        Ok(Self {
            base_url,
            username: username.to_string(),
            password: password.to_string(),
            http,
        })
    }

    /// Build URL with authentication parameters
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

    /// Make an API request and parse the response
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

    /// Test connection to the server
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

    /// Get all artists
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

    /// Get artist details with albums
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

    /// Get album details with songs
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

    /// Get all playlists
    pub async fn get_playlists(&self) -> Result<Vec<Playlist>, SubsonicError> {
        let data: PlaylistsData = self.request("getPlaylists").await?;
        let playlists = data.playlists.playlist;
        debug!("Fetched {} playlists", playlists.len());
        Ok(playlists)
    }

    /// Get playlist details with songs
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

    /// Get stream URL for a song
    ///
    /// Returns the full URL with authentication that can be passed to MPV
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
