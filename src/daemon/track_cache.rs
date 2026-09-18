//! Offline track cache: on-disk audio keyed by song id, an LRU index, and
//! best-effort population from a stream URL.
//!
//! Files and the index live under `$XDG_CACHE_HOME/ferrosonic/tracks`. A track
//! is only indexed once its `.part` download has been renamed into place, so a
//! crash can never leave a partial file counted as cached.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

const INDEX_NAME: &str = "index.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CacheEntry {
    file: String,
    size: u64,
    last_used: u64,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct CacheIndex {
    #[serde(default)]
    entries: HashMap<String, CacheEntry>,
}

/// On-disk track cache with an in-memory LRU index.
pub struct TrackCache {
    dir: PathBuf,
    index: CacheIndex,
    in_flight: HashSet<String>,
    total_bytes: u64,
}

impl TrackCache {
    /// Open the cache at `dir`, loading and reconciling the index. Entries
    /// whose file disappeared are dropped. Does not create the directory
    /// until the first write, so a disabled cache is side-effect free.
    #[must_use]
    pub fn open(dir: PathBuf) -> Self {
        let index_path = dir.join(INDEX_NAME);
        let mut index: CacheIndex = std::fs::read(&index_path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        index
            .entries
            .retain(|_, entry| dir.join(&entry.file).is_file());
        let total_bytes = index.entries.values().map(|entry| entry.size).sum();
        Self {
            dir,
            index,
            in_flight: HashSet::new(),
            total_bytes,
        }
    }

    /// Directory backing the cache.
    #[must_use]
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Path of a cached track, touching its LRU timestamp. `None` when the
    /// track is not cached or its file vanished.
    pub fn path_for(&mut self, song_id: &str) -> Option<PathBuf> {
        let entry = self.index.entries.get_mut(song_id)?;
        let path = self.dir.join(&entry.file);
        if !path.is_file() {
            return None;
        }
        entry.last_used = now_secs();
        Some(path)
    }

    /// Whether `song_id` is present in the index (without touching LRU).
    #[must_use]
    pub fn is_cached(&self, song_id: &str) -> bool {
        self.index.entries.contains_key(song_id)
    }

    /// Reserve a download slot, returning `(part, final, file_name)`. `None`
    /// when the track is already cached or a download is already in flight.
    pub fn begin(&mut self, song_id: &str) -> Option<(PathBuf, PathBuf, String)> {
        if self.index.entries.contains_key(song_id) || self.in_flight.contains(song_id) {
            return None;
        }
        self.in_flight.insert(song_id.to_string());
        let file = file_name_for(song_id);
        let final_path = self.dir.join(&file);
        let part_path = self.dir.join(format!("{file}.part"));
        Some((part_path, final_path, file))
    }

    /// Record a completed download and persist the index.
    pub fn commit(&mut self, song_id: &str, file: String, size: u64) {
        self.in_flight.remove(song_id);
        self.total_bytes = self.total_bytes.saturating_add(size);
        self.index.entries.insert(
            song_id.to_string(),
            CacheEntry {
                file,
                size,
                last_used: now_secs(),
            },
        );
        self.persist();
    }

    /// Release an abandoned download slot (failure or cancellation).
    pub fn abandon(&mut self, song_id: &str) {
        self.in_flight.remove(song_id);
    }

    /// Delete least-recently-used tracks until the cache fits `max_bytes`.
    pub fn evict_to(&mut self, max_bytes: u64) {
        if self.total_bytes <= max_bytes {
            return;
        }
        let mut by_age: Vec<(String, u64, String, u64)> = self
            .index
            .entries
            .iter()
            .map(|(id, entry)| (id.clone(), entry.last_used, entry.file.clone(), entry.size))
            .collect();
        by_age.sort_by_key(|(_, last_used, _, _)| *last_used);
        let mut removed = false;
        for (id, _, file, size) in by_age {
            if self.total_bytes <= max_bytes {
                break;
            }
            let _ = std::fs::remove_file(self.dir.join(file));
            self.index.entries.remove(&id);
            self.total_bytes = self.total_bytes.saturating_sub(size);
            removed = true;
        }
        if removed {
            self.persist();
        }
    }

    fn persist(&self) {
        if let Ok(body) = serde_json::to_vec(&self.index) {
            let _ = crate::io_util::atomic_write_bytes(&self.dir.join(INDEX_NAME), &body);
        }
    }
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// Filesystem-safe name for a song id, with a hash suffix so distinct ids that
/// sanitize to the same prefix cannot collide.
fn file_name_for(song_id: &str) -> String {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    song_id.hash(&mut hasher);
    let safe: String = song_id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .take(64)
        .collect();
    format!("{safe}-{:016x}.audio", hasher.finish())
}

/// Stream `url` to `path`, fsyncing before returning the byte count. The
/// caller renames into place only after this succeeds.
///
/// # Errors
/// Returns an error if the request fails, the response is non-2xx, or the
/// file cannot be written.
pub async fn download_to_path(url: &str, path: &Path) -> std::io::Result<u64> {
    use futures::StreamExt;
    use tokio::io::AsyncWriteExt;

    let client = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(10))
        .timeout(std::time::Duration::from_mins(5))
        .build()
        .map_err(std::io::Error::other)?;
    let response = client
        .get(url)
        .send()
        .await
        .map_err(std::io::Error::other)?;
    if !response.status().is_success() {
        return Err(std::io::Error::other(format!(
            "stream returned HTTP {}",
            response.status()
        )));
    }
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let mut file = tokio::fs::File::create(path).await?;
    let mut written: u64 = 0;
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(std::io::Error::other)?;
        file.write_all(&chunk).await?;
        written = written.saturating_add(u64::try_from(chunk.len()).unwrap_or(u64::MAX));
    }
    file.flush().await?;
    file.sync_all().await?;
    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::{TrackCache, INDEX_NAME};

    fn temp_cache() -> (tempfile::TempDir, TrackCache) {
        let dir = tempfile::tempdir().unwrap();
        let cache = TrackCache::open(dir.path().to_path_buf());
        (dir, cache)
    }

    #[test]
    fn begin_commit_then_lookup_round_trips() {
        let (_dir, mut cache) = temp_cache();
        let (part, final_path, file) = cache.begin("song-1").unwrap();
        std::fs::write(&part, b"audio-bytes").unwrap();
        std::fs::rename(&part, &final_path).unwrap();
        cache.commit("song-1", file, 11);

        assert_eq!(
            cache.path_for("song-1").as_deref(),
            Some(final_path.as_path())
        );
        assert!(cache.begin("song-1").is_none(), "already cached");
    }

    #[test]
    fn begin_dedupes_in_flight_downloads() {
        let (_dir, mut cache) = temp_cache();
        assert!(cache.begin("song-1").is_some());
        assert!(cache.begin("song-1").is_none(), "in-flight dedupe");
        cache.abandon("song-1");
        assert!(cache.begin("song-1").is_some(), "released slot is reusable");
    }

    #[test]
    fn eviction_removes_least_recently_used() {
        let (_dir, mut cache) = temp_cache();
        for (id, used) in [("old", 1u64), ("new", 2u64)] {
            let (part, final_path, file) = cache.begin(id).unwrap();
            std::fs::write(&part, vec![0u8; 10]).unwrap();
            std::fs::rename(&part, &final_path).unwrap();
            cache.commit(id, file, 10);
            // Force deterministic ages for ageing.
            let entry = cache.index.entries.get_mut(id).unwrap();
            entry.last_used = used;
        }
        cache.evict_to(10);
        assert!(cache.is_cached("new"), "newest survives");
        assert!(!cache.is_cached("old"), "oldest is evicted");
    }

    #[test]
    fn index_is_rebuilt_when_the_file_is_missing() {
        let dir = tempfile::tempdir().unwrap();
        // A corrupt index must not panic; the cache comes up empty.
        std::fs::write(dir.path().join(INDEX_NAME), b"{not json").unwrap();
        let mut cache = TrackCache::open(dir.path().to_path_buf());
        assert!(cache.path_for("song-1").is_none());
    }
}
