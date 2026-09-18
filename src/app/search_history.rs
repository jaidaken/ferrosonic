//! Recent search-query history, persisted TUI-side.
//!
//! Deliberately not part of the daemon config: queries are a TUI convenience,
//! and recording one per keystroke would otherwise churn `config.toml`.

use crate::io_util::atomic_write_bytes;

/// Maximum number of remembered queries.
pub const MAX_ENTRIES: usize = 20;

/// Load the persisted history; empty when absent, unreadable, or corrupt.
#[must_use]
pub fn load() -> Vec<String> {
    let Some(path) = crate::config::paths::search_history_file() else {
        return Vec::new();
    };
    let Ok(bytes) = std::fs::read(&path) else {
        return Vec::new();
    };
    serde_json::from_slice::<Vec<String>>(&bytes).unwrap_or_default()
}

/// Record `query` at the front, dropping a case-insensitive duplicate and
/// trimming to [`MAX_ENTRIES`]. Blank queries are ignored.
pub fn record(history: &mut Vec<String>, query: &str) {
    let query = query.trim();
    if query.is_empty() {
        return;
    }
    history.retain(|existing| !existing.eq_ignore_ascii_case(query));
    history.insert(0, query.to_string());
    history.truncate(MAX_ENTRIES);
}

/// Persist `history`. Best-effort: a write failure is logged, never fatal.
pub fn save(history: &[String]) {
    let Some(path) = crate::config::paths::search_history_file() else {
        return;
    };
    let Ok(body) = serde_json::to_vec(history) else {
        return;
    };
    if let Err(e) = atomic_write_bytes(&path, &body) {
        tracing::warn!("search history save failed: {}", e);
    }
}

#[cfg(test)]
mod tests {
    use super::{record, MAX_ENTRIES};

    #[test]
    fn record_moves_duplicates_to_the_front_case_insensitively() {
        let mut h = vec!["alpha".to_string(), "beta".to_string()];
        record(&mut h, "ALPHA");
        assert_eq!(h, vec!["ALPHA", "beta"]);
    }

    #[test]
    fn record_ignores_blank_queries() {
        let mut h = vec!["alpha".to_string()];
        record(&mut h, "   ");
        assert_eq!(h, vec!["alpha"]);
    }

    #[test]
    fn record_caps_at_max_entries() {
        let mut h = Vec::new();
        for i in 0..(MAX_ENTRIES + 5) {
            record(&mut h, &format!("q{i}"));
        }
        assert_eq!(h.len(), MAX_ENTRIES);
        assert_eq!(h[0], format!("q{}", MAX_ENTRIES + 4));
    }
}
