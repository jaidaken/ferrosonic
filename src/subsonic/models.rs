//! Subsonic API response models.

use serde::{Deserialize, Serialize};

/// Top-level envelope every Subsonic endpoint wraps its payload in.
#[derive(Debug, Deserialize)]
pub struct SubsonicResponse<T> {
    /// The single `subsonic-response` object.
    #[serde(rename = "subsonic-response")]
    pub subsonic_response: SubsonicResponseInner<T>,
}

/// Body of the `subsonic-response` envelope.
#[derive(Debug, Deserialize)]
pub struct SubsonicResponseInner<T> {
    /// `"ok"` or `"failed"`.
    pub status: String,
    /// Subsonic API version the server speaks.
    pub version: String,
    /// Error detail when `status` is `"failed"`.
    #[serde(default)]
    pub error: Option<ApiError>,
    /// Endpoint-specific payload, flattened into the envelope.
    #[serde(flatten)]
    pub data: Option<T>,
}

/// Payload of `getOpenSubsonicExtensions`: the extensions the server supports.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct OpenSubsonicExtensionsData {
    /// Supported extensions; empty on a non-OpenSubsonic server.
    #[serde(rename = "openSubsonicExtensions", default)]
    pub extensions: Vec<OpenSubsonicExtension>,
}

/// One advertised OpenSubsonic extension.
#[derive(Debug, Clone, Deserialize)]
pub struct OpenSubsonicExtension {
    /// Extension key, e.g. `playbackReport`.
    pub name: String,
    /// Supported versions of the extension.
    #[serde(default)]
    pub versions: Vec<i32>,
}

/// Error object returned when a Subsonic call fails.
#[derive(Debug, Deserialize)]
pub struct ApiError {
    /// Subsonic error code.
    pub code: i32,
    /// Human-readable error message.
    pub message: String,
}

/// Payload of `getStarred2`.
#[derive(Debug, Deserialize)]
pub struct StarredSongsData {
    /// The `starred2` result object.
    #[serde(rename = "starred2")]
    pub starred_songs: StarredSongs,
}

/// Starred items list inside `getStarred2`.
#[derive(Debug, Deserialize)]
pub struct StarredSongs {
    /// Starred songs; empty when none are starred.
    #[serde(default)]
    pub song: Vec<Child>,
}

/// Payload of `getRandomSongs`.
#[derive(Debug, Deserialize)]
pub struct RandomSongsData {
    /// The `randomSongs` result object.
    #[serde(rename = "randomSongs")]
    pub random_songs: RandomSongs,
}

/// Random songs list inside `getRandomSongs`.
#[derive(Debug, Deserialize)]
pub struct RandomSongs {
    /// Returned songs; empty when the library is empty.
    #[serde(default)]
    pub song: Vec<Child>,
}

/// Payload of `getArtists`.
#[derive(Debug, Deserialize)]
pub struct ArtistsData {
    /// The alphabetical artist index.
    pub artists: ArtistsIndex,
}

/// Alphabetical index buckets of `getArtists`.
#[derive(Debug, Deserialize)]
pub struct ArtistsIndex {
    /// One bucket per index letter.
    #[serde(default)]
    pub index: Vec<ArtistIndex>,
}

/// One index-letter bucket of artists.
#[derive(Debug, Deserialize)]
pub struct ArtistIndex {
    /// Index letter, e.g. `"A"`.
    pub name: String,
    /// Artists under this letter.
    #[serde(default)]
    pub artist: Vec<Artist>,
}

/// Artist summary as listed in the index and search results.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Artist {
    /// Unique Subsonic ID.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Number of albums by this artist, when the server reports it.
    #[serde(default, rename = "albumCount")]
    pub album_count: Option<i32>,
    /// Cover art ID usable with `getCoverArt`.
    #[serde(default, rename = "coverArt")]
    pub cover_art: Option<String>,
}

/// Payload of `getArtist`.
#[derive(Debug, Deserialize)]
pub struct ArtistData {
    /// The requested artist with albums inlined.
    pub artist: ArtistDetail,
}

/// Artist detail returned by `getArtist`.
#[derive(Debug, Deserialize)]
pub struct ArtistDetail {
    /// Unique Subsonic ID.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Albums by this artist.
    #[serde(default)]
    pub album: Vec<Album>,
}

/// Album summary as listed under an artist and in search results.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Album {
    /// Unique Subsonic ID.
    pub id: String,
    /// Album title.
    pub name: String,
    /// Album artist name.
    #[serde(default)]
    pub artist: Option<String>,
    /// ID of the album artist.
    #[serde(default, rename = "artistId")]
    pub artist_id: Option<String>,
    /// Cover art ID usable with `getCoverArt`.
    #[serde(default, rename = "coverArt")]
    pub cover_art: Option<String>,
    /// Number of songs on the album.
    #[serde(default, rename = "songCount")]
    pub song_count: Option<i32>,
    /// Total album duration in seconds.
    #[serde(default)]
    pub duration: Option<i32>,
    /// Release year (tagged; may be a remaster year).
    #[serde(default)]
    pub year: Option<i32>,
    /// OpenSubsonic original release date; preferred over `year` for sorting.
    #[serde(default, rename = "originalReleaseDate")]
    pub original_release_date: Option<ItemDate>,
    /// Genre label.
    #[serde(default)]
    pub genre: Option<String>,
}

/// OpenSubsonic partial date (any component may be absent).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ItemDate {
    /// Year component.
    #[serde(default)]
    pub year: Option<i32>,
    /// Month component, 1-12.
    #[serde(default)]
    pub month: Option<i32>,
    /// Day component, 1-31.
    #[serde(default)]
    pub day: Option<i32>,
}

impl Album {
    /// Year to sort by: the original release year when the server provides it,
    /// else the tagged `year`. `None` sorts last.
    ///
    /// ```
    /// use ferrosonic::subsonic::models::{Album, ItemDate};
    /// let mut a = Album { id: "1".into(), name: "x".into(), artist: None,
    ///     artist_id: None, cover_art: None, song_count: None, duration: None,
    ///     year: Some(2015), original_release_date: Some(ItemDate {
    ///         year: Some(1979), month: None, day: None }), genre: None };
    /// assert_eq!(a.sort_year(), Some(1979));
    /// a.original_release_date = None;
    /// assert_eq!(a.sort_year(), Some(2015));
    /// ```
    #[must_use]
    pub fn sort_year(&self) -> Option<i32> {
        self.original_release_date
            .as_ref()
            .and_then(|d| d.year)
            .or(self.year)
    }
}

/// `getAlbumList2` response payload.
#[derive(Debug, Clone, Deserialize)]
pub struct AlbumList2Data {
    /// The album list under `albumList2`.
    #[serde(rename = "albumList2", default)]
    pub album_list2: AlbumList2,
}

/// The `albumList2` object holding the album array.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct AlbumList2 {
    /// Albums in the requested sort order and page.
    #[serde(default)]
    pub album: Vec<Album>,
}

/// Payload of `getAlbum`.
#[derive(Debug, Deserialize)]
pub struct AlbumData {
    /// The requested album with songs inlined.
    pub album: AlbumDetail,
}

/// Album detail returned by `getAlbum`.
#[derive(Debug, Deserialize)]
pub struct AlbumDetail {
    /// Unique Subsonic ID.
    pub id: String,
    /// Album title.
    pub name: String,
    /// Album artist name.
    #[serde(default)]
    pub artist: Option<String>,
    /// ID of the album artist.
    #[serde(default, rename = "artistId")]
    pub artist_id: Option<String>,
    /// Release year.
    #[serde(default)]
    pub year: Option<i32>,
    /// Songs on the album, in track order.
    #[serde(default)]
    pub song: Vec<Child>,
}

/// Song / media item. Subsonic calls this `Child`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Child {
    /// Unique Subsonic ID.
    pub id: String,
    /// ID of the containing directory or album.
    #[serde(default)]
    pub parent: Option<String>,
    /// Whether this entry is a directory rather than a song.
    #[serde(default, rename = "isDir")]
    pub is_dir: bool,
    /// Song title.
    pub title: String,
    /// Album title.
    #[serde(default)]
    pub album: Option<String>,
    /// Artist name.
    #[serde(default)]
    pub artist: Option<String>,
    /// ID of the song's artist, when the server provides it.
    #[serde(default, rename = "artistId")]
    pub artist_id: Option<String>,
    /// Track number within the disc.
    #[serde(default)]
    pub track: Option<i32>,
    /// Release year.
    #[serde(default)]
    pub year: Option<i32>,
    /// Genre label.
    #[serde(default)]
    pub genre: Option<String>,
    /// Cover art ID usable with `getCoverArt`.
    #[serde(default, rename = "coverArt")]
    pub cover_art: Option<String>,
    /// File size in bytes.
    #[serde(default)]
    pub size: Option<i64>,
    /// MIME type of the media file.
    #[serde(default, rename = "contentType")]
    pub content_type: Option<String>,
    /// File extension, e.g. `"flac"`.
    #[serde(default)]
    pub suffix: Option<String>,
    /// Duration in seconds.
    #[serde(default)]
    pub duration: Option<i32>,
    /// Bit rate in kbit/s.
    #[serde(default, rename = "bitRate")]
    pub bit_rate: Option<i32>,
    /// Server-side file path.
    #[serde(default)]
    pub path: Option<String>,
    /// Disc number for multi-disc albums.
    #[serde(default, rename = "discNumber")]
    pub disc_number: Option<i32>,
    /// Star timestamp; present only when the song is starred.
    #[serde(default)]
    pub starred: Option<String>,
}

impl Child {
    /// Duration as `MM:SS`, or `--:--` when unknown.
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

/// Payload of `getPlaylists`.
#[derive(Debug, Deserialize)]
pub struct PlaylistsData {
    /// The playlists wrapper object.
    pub playlists: PlaylistsInner,
}

/// Playlist list inside `getPlaylists`.
#[derive(Debug, Deserialize)]
pub struct PlaylistsInner {
    /// All playlists visible to the account.
    #[serde(default)]
    pub playlist: Vec<Playlist>,
}

/// Playlist summary as listed by `getPlaylists`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Playlist {
    /// Unique Subsonic ID.
    pub id: String,
    /// Playlist name.
    pub name: String,
    /// Username of the playlist owner.
    #[serde(default)]
    pub owner: Option<String>,
    /// Number of songs in the playlist.
    #[serde(default, rename = "songCount")]
    pub song_count: Option<i32>,
    /// Total playlist duration in seconds.
    #[serde(default)]
    pub duration: Option<i32>,
    /// Cover art ID usable with `getCoverArt`.
    #[serde(default, rename = "coverArt")]
    pub cover_art: Option<String>,
    /// Whether the playlist is shared publicly.
    #[serde(default)]
    pub public: Option<bool>,
    /// Free-form playlist comment.
    #[serde(default)]
    pub comment: Option<String>,
}

/// Payload of `getPlaylist`.
#[derive(Debug, Deserialize)]
pub struct PlaylistData {
    /// The requested playlist with songs inlined.
    pub playlist: PlaylistDetail,
}

/// Playlist detail returned by `getPlaylist`.
#[derive(Debug, Deserialize)]
pub struct PlaylistDetail {
    /// Unique Subsonic ID.
    pub id: String,
    /// Playlist name.
    pub name: String,
    /// Username of the playlist owner.
    #[serde(default)]
    pub owner: Option<String>,
    /// Number of songs in the playlist.
    #[serde(default, rename = "songCount")]
    pub song_count: Option<i32>,
    /// Total playlist duration in seconds.
    #[serde(default)]
    pub duration: Option<i32>,
    /// Songs in playlist order.
    #[serde(default)]
    pub entry: Vec<Child>,
}

/// Empty payload of `ping`.
#[derive(Debug, Deserialize)]
pub struct PingData {}

/// Payload of `search3`.
#[derive(Debug, Deserialize)]
pub struct Search3Data {
    /// The `searchResult3` object; defaults to empty on no matches.
    #[serde(rename = "searchResult3", default)]
    pub result: SearchResult3,
}

/// Combined artist / album / song search results.
#[derive(Debug, Default, Clone, Deserialize, Serialize)]
pub struct SearchResult3 {
    /// Matching artists.
    #[serde(default)]
    pub artist: Vec<Artist>,
    /// Matching albums.
    #[serde(default)]
    pub album: Vec<Album>,
    /// Matching songs.
    #[serde(default)]
    pub song: Vec<Child>,
}
