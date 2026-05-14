//! Subsonic API response models.

use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub struct SubsonicResponse<T> {
    #[serde(rename = "subsonic-response")]
    pub subsonic_response: SubsonicResponseInner<T>,
}

#[derive(Debug, Deserialize)]
pub struct SubsonicResponseInner<T> {
    pub status: String,
    pub version: String,
    #[serde(default)]
    pub error: Option<ApiError>,
    #[serde(flatten)]
    pub data: Option<T>,
}

#[derive(Debug, Deserialize)]
pub struct ApiError {
    pub code: i32,
    pub message: String,
}

#[derive(Debug, Deserialize)]
pub struct StarredSongsData {
    #[serde(rename = "starred2")]
    pub starred_songs: StarredSongs,
}

#[derive(Debug, Deserialize)]
pub struct StarredSongs {
    #[serde(default)]
    pub song: Vec<Child>,
}

#[derive(Debug, Deserialize)]
pub struct RandomSongsData {
    #[serde(rename = "randomSongs")]
    pub random_songs: RandomSongs,
}

#[derive(Debug, Deserialize)]
pub struct RandomSongs {
    #[serde(default)]
    pub song: Vec<Child>,
}

#[derive(Debug, Deserialize)]
pub struct ArtistsData {
    pub artists: ArtistsIndex,
}

#[derive(Debug, Deserialize)]
pub struct ArtistsIndex {
    #[serde(default)]
    pub index: Vec<ArtistIndex>,
}

#[derive(Debug, Deserialize)]
pub struct ArtistIndex {
    pub name: String,
    #[serde(default)]
    pub artist: Vec<Artist>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Artist {
    pub id: String,
    pub name: String,
    #[serde(default, rename = "albumCount")]
    pub album_count: Option<i32>,
    #[serde(default, rename = "coverArt")]
    pub cover_art: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ArtistData {
    pub artist: ArtistDetail,
}

#[derive(Debug, Deserialize)]
pub struct ArtistDetail {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub album: Vec<Album>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Album {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub artist: Option<String>,
    #[serde(default, rename = "artistId")]
    pub artist_id: Option<String>,
    #[serde(default, rename = "coverArt")]
    pub cover_art: Option<String>,
    #[serde(default, rename = "songCount")]
    pub song_count: Option<i32>,
    #[serde(default)]
    pub duration: Option<i32>,
    #[serde(default)]
    pub year: Option<i32>,
    #[serde(default)]
    pub genre: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct AlbumData {
    pub album: AlbumDetail,
}

#[derive(Debug, Deserialize)]
pub struct AlbumDetail {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub artist: Option<String>,
    #[serde(default, rename = "artistId")]
    pub artist_id: Option<String>,
    #[serde(default)]
    pub year: Option<i32>,
    #[serde(default)]
    pub song: Vec<Child>,
}

/// Song / media item. Subsonic calls this `Child`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Child {
    pub id: String,
    #[serde(default)]
    pub parent: Option<String>,
    #[serde(default, rename = "isDir")]
    pub is_dir: bool,
    pub title: String,
    #[serde(default)]
    pub album: Option<String>,
    #[serde(default)]
    pub artist: Option<String>,
    #[serde(default)]
    pub track: Option<i32>,
    #[serde(default)]
    pub year: Option<i32>,
    #[serde(default)]
    pub genre: Option<String>,
    #[serde(default, rename = "coverArt")]
    pub cover_art: Option<String>,
    #[serde(default)]
    pub size: Option<i64>,
    #[serde(default, rename = "contentType")]
    pub content_type: Option<String>,
    #[serde(default)]
    pub suffix: Option<String>,
    #[serde(default)]
    pub duration: Option<i32>,
    #[serde(default, rename = "bitRate")]
    pub bit_rate: Option<i32>,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default, rename = "discNumber")]
    pub disc_number: Option<i32>,
    #[serde(default)]
    pub starred: Option<String>,
}

impl Child {
    pub fn format_duration(&self) -> String {
        match self.duration {
            Some(d) => {
                let mins = d / 60;
                let secs = d % 60;
                format!("{:02}:{:02}", mins, secs)
            }
            None => "--:--".to_string(),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct PlaylistsData {
    pub playlists: PlaylistsInner,
}

#[derive(Debug, Deserialize)]
pub struct PlaylistsInner {
    #[serde(default)]
    pub playlist: Vec<Playlist>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Playlist {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub owner: Option<String>,
    #[serde(default, rename = "songCount")]
    pub song_count: Option<i32>,
    #[serde(default)]
    pub duration: Option<i32>,
    #[serde(default, rename = "coverArt")]
    pub cover_art: Option<String>,
    #[serde(default)]
    pub public: Option<bool>,
    #[serde(default)]
    pub comment: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct PlaylistData {
    pub playlist: PlaylistDetail,
}

#[derive(Debug, Deserialize)]
pub struct PlaylistDetail {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub owner: Option<String>,
    #[serde(default, rename = "songCount")]
    pub song_count: Option<i32>,
    #[serde(default)]
    pub duration: Option<i32>,
    #[serde(default)]
    pub entry: Vec<Child>,
}

#[derive(Debug, Deserialize)]
pub struct PingData {}

#[derive(Debug, Deserialize)]
pub struct Search3Data {
    #[serde(rename = "searchResult3", default)]
    pub result: SearchResult3,
}

#[derive(Debug, Default, Clone, Deserialize, Serialize)]
pub struct SearchResult3 {
    #[serde(default)]
    pub artist: Vec<Artist>,
    #[serde(default)]
    pub album: Vec<Album>,
    #[serde(default)]
    pub song: Vec<Child>,
}
