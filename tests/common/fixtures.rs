//! Builders for common test fixtures.

use ferrosonic::subsonic::models::Child;

/// Minimal song with a stable id and title. Duration 180 s by default.
pub fn song(id: &str, title: &str) -> Child {
    Child {
        id: id.into(),
        title: title.into(),
        parent: None,
        is_dir: false,
        album: Some("Test Album".into()),
        artist: Some("Test Artist".into()),
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
    }
}

/// Build a contiguous run of test songs.
pub fn songs(prefix: &str, count: usize) -> Vec<Child> {
    (0..count)
        .map(|i| song(&format!("{}-{}", prefix, i), &format!("Track {}", i)))
        .collect()
}
