//! Test fixture builders.

use ferrosonic::subsonic::models::Child;

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

pub fn song_starred(id: &str, title: &str) -> Child {
    let mut s = song(id, title);
    s.starred = Some("2024-01-01T00:00:00Z".into());
    s
}

pub fn songs(prefix: &str, count: usize) -> Vec<Child> {
    (0..count)
        .map(|i| song(&format!("{}-{}", prefix, i), &format!("Track {}", i)))
        .collect()
}
