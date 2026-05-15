pub mod paths;

use serde::{Deserialize, Serialize};
use std::path::Path;
use tracing::{debug, info, warn};

use crate::error::ConfigError;
use crate::io_util::{atomic_write_bytes, fsync_parent_dir};
use crate::secret::{serialize_revealed, Secret};

/// All top-level TOML keys we expect. Anything not in this list is
/// warned on load so a typo like `RepeateMode` is visible instead of
/// silently reverting to the default.
pub const KNOWN_CONFIG_KEYS: &[&str] = &[
    "BaseURL",
    "Username",
    "Password",
    "PasswordFile",
    "Theme",
    "Cava",
    "CavaSize",
    "Daemon",
    "AutoContinue",
    "RepeatMode",
    "CoverArt",
    "CoverArtSize",
];

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Config {
    #[serde(rename = "BaseURL", default)]
    pub base_url: String,

    #[serde(rename = "Username", default)]
    pub username: String,

    /// Resolved at load-time from env, PasswordFile, then this inline value. Secret masks Debug + Serialize so accidental log/wire paths emit "***"; save_to_file routes through ConfigOnDisk which writes the real value.
    #[serde(rename = "Password", default)]
    pub password: Secret,

    #[serde(
        rename = "PasswordFile",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub password_file: Option<String>,

    #[serde(rename = "Theme", default)]
    pub theme: String,

    #[serde(rename = "Cava", default)]
    pub cava: bool,

    #[serde(rename = "CavaSize", default = "Config::default_cava_size")]
    pub cava_size: u8,

    /// `false` forces standalone mode on next launch.
    #[serde(rename = "Daemon", default = "Config::default_daemon")]
    pub daemon: bool,

    #[serde(rename = "AutoContinue", default)]
    pub auto_continue: bool,

    #[serde(rename = "RepeatMode", default)]
    pub repeat_mode: RepeatMode,

    #[serde(rename = "CoverArt", default)]
    pub cover_art: bool,

    /// Total height of the now-playing section in rows when cover art
    /// is visible. Range 8..=24, step 2. The art height is this minus
    /// 3 (2 border rows + 1 progress bar row).
    #[serde(rename = "CoverArtSize", default = "Config::default_cover_art_size")]
    pub cover_art_size: u8,
}

#[derive(Serialize)]
struct ConfigOnDisk<'a> {
    #[serde(rename = "BaseURL")]
    base_url: &'a str,
    #[serde(rename = "Username")]
    username: &'a str,
    #[serde(
        rename = "Password",
        serialize_with = "serialize_revealed_opt",
        skip_serializing_if = "Option::is_none"
    )]
    password: Option<&'a Secret>,
    #[serde(rename = "PasswordFile", skip_serializing_if = "Option::is_none")]
    password_file: Option<&'a str>,
    #[serde(rename = "Theme")]
    theme: &'a str,
    #[serde(rename = "Cava")]
    cava: bool,
    #[serde(rename = "CavaSize")]
    cava_size: u8,
    #[serde(rename = "Daemon")]
    daemon: bool,
    #[serde(rename = "AutoContinue")]
    auto_continue: bool,
    #[serde(rename = "RepeatMode")]
    repeat_mode: RepeatMode,
    #[serde(rename = "CoverArt")]
    cover_art: bool,
    #[serde(rename = "CoverArtSize")]
    cover_art_size: u8,
}

fn serialize_revealed_opt<S: serde::Serializer>(
    s: &Option<&Secret>,
    ser: S,
) -> Result<S::Ok, S::Error> {
    match s {
        Some(sec) => serialize_revealed(sec, ser),
        None => ser.serialize_str(""),
    }
}

impl Config {
    fn as_on_disk(&self) -> ConfigOnDisk<'_> {
        let pw_file_set = self
            .password_file
            .as_ref()
            .is_some_and(|s| !s.is_empty());
        ConfigOnDisk {
            base_url: &self.base_url,
            username: &self.username,
            password: if pw_file_set || self.password.is_empty() {
                None
            } else {
                Some(&self.password)
            },
            password_file: self.password_file.as_deref(),
            theme: &self.theme,
            cava: self.cava,
            cava_size: self.cava_size,
            daemon: self.daemon,
            auto_continue: self.auto_continue,
            repeat_mode: self.repeat_mode,
            cover_art: self.cover_art,
            cover_art_size: self.cover_art_size,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum RepeatMode {
    #[default]
    Off,
    One,
    All,
}

impl RepeatMode {
    pub fn label(self) -> &'static str {
        match self {
            RepeatMode::Off => "off",
            RepeatMode::One => "one",
            RepeatMode::All => "all",
        }
    }
    /// Step through `Off -> One -> All -> Off` for UI cycling.
    ///
    /// ```
    /// use ferrosonic::config::RepeatMode;
    /// assert_eq!(RepeatMode::Off.cycle(), RepeatMode::One);
    /// assert_eq!(RepeatMode::One.cycle(), RepeatMode::All);
    /// assert_eq!(RepeatMode::All.cycle(), RepeatMode::Off);
    /// ```
    pub fn cycle(self) -> Self {
        match self {
            RepeatMode::Off => RepeatMode::One,
            RepeatMode::One => RepeatMode::All,
            RepeatMode::All => RepeatMode::Off,
        }
    }
    /// Auto-advance: `One` repeats current, `All` wraps, `Off` returns `None` at the end (caller handles auto-continue / stop).
    ///
    /// ```
    /// use ferrosonic::config::RepeatMode;
    /// assert_eq!(RepeatMode::One.next_auto(2, 5), Some(2));
    /// assert_eq!(RepeatMode::All.next_auto(4, 5), Some(0));
    /// assert_eq!(RepeatMode::Off.next_auto(4, 5), None);
    /// ```
    pub fn next_auto(self, current: usize, queue_len: usize) -> Option<usize> {
        if queue_len == 0 {
            return None;
        }
        match self {
            RepeatMode::One => Some(current),
            RepeatMode::All => Some((current + 1) % queue_len),
            RepeatMode::Off => {
                if current + 1 < queue_len {
                    Some(current + 1)
                } else {
                    None
                }
            }
        }
    }
    /// Manual skip: `One` is ignored - user wants to move.
    ///
    /// ```
    /// use ferrosonic::config::RepeatMode;
    /// assert_eq!(RepeatMode::One.next_manual(4, 5), Some(0));
    /// assert_eq!(RepeatMode::All.next_manual(0, 3), Some(1));
    /// assert_eq!(RepeatMode::Off.next_manual(2, 3), None);
    /// ```
    pub fn next_manual(self, current: usize, queue_len: usize) -> Option<usize> {
        if queue_len == 0 {
            return None;
        }
        match self {
            RepeatMode::All | RepeatMode::One => Some((current + 1) % queue_len),
            RepeatMode::Off => {
                if current + 1 < queue_len {
                    Some(current + 1)
                } else {
                    None
                }
            }
        }
    }
    /// Manual prev from position 0: `All`/`One` wrap to last track, `Off` returns `None` (caller restarts current).
    ///
    /// ```
    /// use ferrosonic::config::RepeatMode;
    /// assert_eq!(RepeatMode::All.prev_wrap(5), Some(4));
    /// assert_eq!(RepeatMode::One.prev_wrap(5), Some(4));
    /// assert_eq!(RepeatMode::Off.prev_wrap(5), None);
    /// ```
    pub fn prev_wrap(self, queue_len: usize) -> Option<usize> {
        if queue_len == 0 {
            return None;
        }
        match self {
            RepeatMode::All | RepeatMode::One => Some(queue_len - 1),
            RepeatMode::Off => None,
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            base_url: String::new(),
            username: String::new(),
            password: Secret::new(),
            password_file: None,
            theme: String::new(),
            cava: false,
            cava_size: Self::default_cava_size(),
            daemon: Self::default_daemon(),
            auto_continue: false,
            repeat_mode: RepeatMode::Off,
            cover_art: false,
            cover_art_size: Self::default_cover_art_size(),
        }
    }
}

impl Config {
    fn default_cava_size() -> u8 {
        40
    }

    fn default_daemon() -> bool {
        true
    }

    fn default_cover_art_size() -> u8 {
        16
    }

    pub fn new() -> Self {
        Self::default()
    }

    pub fn load_default() -> Result<Self, ConfigError> {
        let path = paths::config_file().ok_or_else(|| ConfigError::NotFound {
            path: "default config location".to_string(),
        })?;

        if path.exists() {
            Self::load_from_file(&path)
        } else {
            info!("No config file found at {}, using defaults", path.display());
            Ok(Self::new())
        }
    }

    /// Resolves the password in priority order: `FERROSONIC_PASSWORD` env > `PasswordFile` > inline.
    ///
    /// ```
    /// use ferrosonic::config::Config;
    /// use ferrosonic::io_util::atomic_write_bytes;
    /// let dir = tempfile::tempdir().unwrap();
    /// let p = dir.path().join("c.toml");
    /// atomic_write_bytes(&p, b"BaseURL = \"https://x\"\n").unwrap();
    /// let c = Config::load_from_file(&p).unwrap();
    /// assert_eq!(c.base_url, "https://x");
    /// ```
    pub fn load_from_file(path: &Path) -> Result<Self, ConfigError> {
        debug!("Loading config from {}", path.display());

        if !path.exists() {
            return Err(ConfigError::NotFound {
                path: path.display().to_string(),
            });
        }

        let contents = std::fs::read_to_string(path)?;
        let mut config: Config = toml::from_str(&contents)?;
        config.resolve_password();
        // Warn on unknown top-level keys so typos like `RepeateMode`
        // don't silently revert to the default.
        if let Ok(val) = toml::from_str::<toml::Value>(&contents) {
            if let Some(table) = val.as_table() {
                for k in table.keys() {
                    if !KNOWN_CONFIG_KEYS.contains(&k.as_str()) {
                        warn!("Unknown config key: {} (typo? value ignored)", k);
                    }
                }
            }
        }

        debug!("Config loaded successfully");
        Ok(config)
    }

    /// Expand `~/` if present in a password-file path.
    ///
    /// ```
    /// use ferrosonic::config::Config;
    /// assert_eq!(Config::expand_tilde("/etc/passwd"), "/etc/passwd");
    /// assert_eq!(Config::expand_tilde(""), "");
    /// ```
    pub fn expand_tilde(path: &str) -> String {
        if let Some(rest) = path.strip_prefix("~/") {
            if let Ok(home) = std::env::var("HOME") {
                return format!("{}/{}", home, rest);
            }
        }
        path.to_string()
    }

    fn resolve_password(&mut self) {
        if let Ok(env) = std::env::var("FERROSONIC_PASSWORD") {
            if !env.is_empty() {
                debug!("Using password from FERROSONIC_PASSWORD env var");
                self.password = Secret::from_string(env);
                return;
            }
        }
        if let Some(pf) = self.password_file.as_ref().filter(|s| !s.is_empty()) {
            let expanded = Self::expand_tilde(pf);
            match std::fs::read_to_string(&expanded) {
                Ok(mut contents) => {
                    debug!("Using password from {}", expanded);
                    let trimmed = contents
                        .trim_end_matches(['\n', '\r', ' ', '\t'])
                        .to_string();
                    use zeroize::Zeroize;
                    contents.zeroize();
                    self.password = Secret::from_string(trimmed);
                }
                Err(e) => {
                    warn!(
                        "PasswordFile {} unreadable ({}); clearing inline password to avoid silent fallback to stale credentials",
                        expanded, e
                    );
                    self.password.clear();
                }
            }
        }
    }

    pub fn save_default(&self) -> Result<(), ConfigError> {
        let path = paths::config_file().ok_or_else(|| ConfigError::NotFound {
            path: "default config location".to_string(),
        })?;

        self.save_to_file(&path)
    }

    /// Atomically write the config TOML; round-trips via load_from_file.
    ///
    /// ```
    /// use ferrosonic::config::Config;
    /// let dir = tempfile::tempdir().unwrap();
    /// let p = dir.path().join("c.toml");
    /// let mut c = Config::new();
    /// c.base_url = "https://x".into();
    /// c.save_to_file(&p).unwrap();
    /// assert_eq!(Config::load_from_file(&p).unwrap().base_url, "https://x");
    /// ```
    pub fn save_to_file(&self, path: &Path) -> Result<(), ConfigError> {
        debug!("Saving config to {}", path.display());
        // ConfigOnDisk uses the real password and obeys password_file indirection so neither the redacted-serializer nor a caller mistake can leak or omit the secret.
        let contents = toml::to_string_pretty(&self.as_on_disk())?;
        atomic_write_bytes(path, contents.as_bytes())?;
        info!("Config saved to {}", path.display());
        Ok(())
    }

    /// True when base_url, username, and password are all non-empty.
    ///
    /// ```
    /// use ferrosonic::config::Config;
    /// use ferrosonic::secret::Secret;
    /// let mut c = Config::new();
    /// assert!(!c.is_configured());
    /// c.base_url = "https://x".into();
    /// c.username = "u".into();
    /// c.password = Secret::from("p");
    /// assert!(c.is_configured());
    /// ```
    pub fn is_configured(&self) -> bool {
        !self.base_url.is_empty() && !self.username.is_empty() && !self.password.is_empty()
    }

    pub fn password_str(&self) -> &str {
        self.password.reveal()
    }

    /// Reject empty or malformed base_url. Empty username/password warn only.
    ///
    /// ```
    /// use ferrosonic::config::Config;
    /// assert!(Config::new().validate().is_err());
    /// let mut c = Config::new();
    /// c.base_url = "https://x".into();
    /// assert!(c.validate().is_ok());
    /// ```
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.base_url.is_empty() {
            return Err(ConfigError::MissingField {
                field: "BaseURL".to_string(),
            });
        }

        if url::Url::parse(&self.base_url).is_err() {
            return Err(ConfigError::InvalidUrl {
                url: self.base_url.clone(),
            });
        }

        if self.username.is_empty() {
            warn!("Username is empty");
        }

        if self.password.is_empty() {
            warn!("Password is empty");
        }

        Ok(())
    }
}

/// Atomic password-file writer: temp + rename + 0600 + parent dir fsync.
pub fn write_password_file_atomic(
    path: &str,
    password: &Secret,
) -> std::io::Result<()> {
    use std::io::Write;
    let expanded = Config::expand_tilde(path);
    let p = Path::new(&expanded);
    if let Some(parent) = p.parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            std::fs::create_dir_all(parent)?;
        }
    }
    let tmp = p.with_extension("tmp");
    let mut opts = std::fs::OpenOptions::new();
    opts.create(true).write(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let mut f = opts.open(&tmp)?;
    f.write_all(password.reveal_bytes())?;
    f.write_all(b"\n")?;
    f.sync_all()?;
    drop(f);
    std::fs::rename(&tmp, p)?;
    fsync_parent_dir(p);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_config_parse() {
        let toml_content = r#"
BaseURL = "https://example.com"
Username = "testuser"
Password = "testpass"
"#;

        let mut file = NamedTempFile::new().unwrap();
        file.write_all(toml_content.as_bytes()).unwrap();

        let config = Config::load_from_file(file.path()).unwrap();
        assert_eq!(config.base_url, "https://example.com");
        assert_eq!(config.username, "testuser");
        assert_eq!(config.password_str(), "testpass");
    }

    #[test]
    fn test_is_configured() {
        let mut config = Config::new();
        assert!(!config.is_configured());

        config.base_url = "https://example.com".to_string();
        config.username = "user".to_string();
        config.password = Secret::from_string("pass".to_string());
        assert!(config.is_configured());
    }

    #[test]
    fn defaults_match_documented_values() {
        let c = Config::default();
        assert_eq!(c.cava_size, 40);
        assert_eq!(c.cover_art_size, 16);
        assert!(c.daemon, "daemon defaults on");
        assert!(!c.cava);
        assert!(!c.cover_art);
        assert!(!c.auto_continue);
        assert_eq!(c.repeat_mode, RepeatMode::Off);
    }

    #[test]
    fn missing_fields_fall_back_to_defaults() {
        let toml = "BaseURL = \"https://x\"\n";
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(toml.as_bytes()).unwrap();
        let c = Config::load_from_file(file.path()).unwrap();
        assert_eq!(c.base_url, "https://x");
        assert_eq!(c.cava_size, 40, "CavaSize falls back");
        assert_eq!(c.cover_art_size, 16, "CoverArtSize falls back");
        assert!(c.daemon, "Daemon defaults true");
    }

    #[test]
    fn corrupt_toml_returns_error() {
        let toml = "this is not valid = = toml [[";
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(toml.as_bytes()).unwrap();
        let r = Config::load_from_file(file.path());
        assert!(r.is_err(), "corrupt TOML should not parse");
    }

    #[test]
    fn unknown_field_is_ignored_not_fatal() {
        let toml = "BaseURL = \"x\"\nUnknownKey = 5\n";
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(toml.as_bytes()).unwrap();
        let c = Config::load_from_file(file.path()).expect("unknown fields tolerated");
        assert_eq!(c.base_url, "x");
    }

    #[test]
    fn repeat_mode_serializes_in_pascal_case() {
        for (mode, expected) in [
            (RepeatMode::Off, "\"Off\""),
            (RepeatMode::One, "\"One\""),
            (RepeatMode::All, "\"All\""),
        ] {
            let s = toml::Value::try_from(mode).unwrap();
            assert_eq!(
                s.to_string(),
                expected,
                "{:?} serializes as {}",
                mode,
                expected
            );
        }
    }

    #[test]
    fn cover_art_size_round_trip_preserved() {
        let toml = "BaseURL = \"x\"\nCoverArtSize = 22\n";
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(toml.as_bytes()).unwrap();
        let c = Config::load_from_file(file.path()).unwrap();
        assert_eq!(c.cover_art_size, 22);
    }

    #[test]
    fn repeat_mode_explicit_value_loads() {
        let toml = "BaseURL = \"x\"\nRepeatMode = \"All\"\n";
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(toml.as_bytes()).unwrap();
        let c = Config::load_from_file(file.path()).unwrap();
        assert_eq!(c.repeat_mode, RepeatMode::All);
    }

    #[test]
    fn cycle_visits_all_three_modes() {
        assert_eq!(RepeatMode::Off.cycle(), RepeatMode::One);
        assert_eq!(RepeatMode::One.cycle(), RepeatMode::All);
        assert_eq!(RepeatMode::All.cycle(), RepeatMode::Off);
    }

    #[test]
    fn labels_are_lowercase_words() {
        assert_eq!(RepeatMode::Off.label(), "off");
        assert_eq!(RepeatMode::One.label(), "one");
        assert_eq!(RepeatMode::All.label(), "all");
    }

    #[test]
    fn next_manual_off_advances_then_stops_at_end() {
        let mode = RepeatMode::Off;
        assert_eq!(mode.next_manual(0, 3), Some(1));
        assert_eq!(mode.next_manual(1, 3), Some(2));
        assert_eq!(
            mode.next_manual(2, 3),
            None,
            "Off does not wrap on manual Next"
        );
    }

    #[test]
    fn next_manual_all_wraps_at_end() {
        let mode = RepeatMode::All;
        assert_eq!(mode.next_manual(0, 3), Some(1));
        assert_eq!(mode.next_manual(2, 3), Some(0), "All wraps at end");
    }

    #[test]
    fn next_manual_one_still_advances_on_manual_skip() {
        let mode = RepeatMode::One;
        assert_eq!(
            mode.next_manual(0, 3),
            Some(1),
            "manual Next under repeat-One should still move forward"
        );
        assert_eq!(
            mode.next_manual(2, 3),
            Some(0),
            "repeat-One wraps on manual Next"
        );
    }

    #[test]
    fn next_auto_off_advances_then_stops_at_end() {
        let mode = RepeatMode::Off;
        assert_eq!(mode.next_auto(0, 3), Some(1));
        assert_eq!(mode.next_auto(1, 3), Some(2));
        assert_eq!(
            mode.next_auto(2, 3),
            None,
            "Off returns None at end so the caller can trigger auto-continue or stop"
        );
    }

    #[test]
    fn next_auto_all_wraps_at_end() {
        let mode = RepeatMode::All;
        assert_eq!(mode.next_auto(2, 3), Some(0), "All wraps on auto-advance");
    }

    #[test]
    fn next_auto_one_repeats_current_track() {
        let mode = RepeatMode::One;
        assert_eq!(
            mode.next_auto(0, 3),
            Some(0),
            "repeat-One repeats the same index on auto-advance"
        );
        assert_eq!(mode.next_auto(2, 3), Some(2));
    }

    #[test]
    fn next_handlers_return_none_on_empty_queue() {
        for mode in [RepeatMode::Off, RepeatMode::One, RepeatMode::All] {
            assert_eq!(
                mode.next_manual(0, 0),
                None,
                "{:?} manual on empty queue",
                mode
            );
            assert_eq!(mode.next_auto(0, 0), None, "{:?} auto on empty queue", mode);
        }
    }

    #[test]
    fn prev_wrap_off_returns_none_at_start() {
        assert_eq!(
            RepeatMode::Off.prev_wrap(3),
            None,
            "Off does not wrap on Previous from position 0"
        );
    }

    #[test]
    fn prev_wrap_all_and_one_wrap_to_last_track() {
        assert_eq!(RepeatMode::All.prev_wrap(3), Some(2));
        assert_eq!(RepeatMode::One.prev_wrap(3), Some(2));
    }

    #[test]
    fn prev_wrap_empty_queue_returns_none() {
        for mode in [RepeatMode::Off, RepeatMode::One, RepeatMode::All] {
            assert_eq!(mode.prev_wrap(0), None);
        }
    }
}
