pub mod paths;

use serde::{Deserialize, Serialize};
use std::path::Path;
use tracing::{debug, info, warn};

use crate::error::ConfigError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(rename = "BaseURL", default)]
    pub base_url: String,

    #[serde(rename = "Username", default)]
    pub username: String,

    /// Resolved at load-time from `FERROSONIC_PASSWORD` env, then
    /// `PasswordFile`, then this inline value.
    #[serde(rename = "Password", default)]
    pub password: String,

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
    pub fn cycle(self) -> Self {
        match self {
            RepeatMode::Off => RepeatMode::One,
            RepeatMode::One => RepeatMode::All,
            RepeatMode::All => RepeatMode::Off,
        }
    }
    /// Auto-advance: `One` repeats current, `All` wraps, `Off`
    /// returns `None` at the end (caller handles auto-continue / stop).
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
    /// Manual skip: `One` is ignored — user wants to move.
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
    /// Manual prev from position 0: `All`/`One` wrap to last track,
    /// `Off` returns `None` (caller restarts current).
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
            password: String::new(),
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

    /// Resolves the password in priority order: `FERROSONIC_PASSWORD`
    /// env > `PasswordFile` > inline.
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

        debug!("Config loaded successfully");
        Ok(config)
    }

    /// Expand `~/` if present in a password-file path.
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
                self.password = env;
                return;
            }
        }
        if let Some(pf) = self.password_file.as_ref().filter(|s| !s.is_empty()) {
            let expanded = Self::expand_tilde(pf);
            match std::fs::read_to_string(&expanded) {
                Ok(contents) => {
                    debug!("Using password from {}", expanded);
                    self.password = contents
                        .trim_end_matches(['\n', '\r', ' ', '\t'])
                        .to_string();
                }
                Err(e) => {
                    warn!(
                        "PasswordFile {} unreadable: {}; falling back to inline password",
                        expanded, e
                    );
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

    pub fn save_to_file(&self, path: &Path) -> Result<(), ConfigError> {
        debug!("Saving config to {}", path.display());

        if let Some(parent) = path.parent() {
            if !parent.exists() {
                std::fs::create_dir_all(parent)?;
            }
        }

        let contents = toml::to_string_pretty(self)?;
        // Write-temp-then-rename so a crash mid-write cannot leave a
        // truncated config.toml and wipe the user's credentials.
        let tmp = path.with_extension("toml.tmp");
        std::fs::write(&tmp, contents)?;
        std::fs::rename(&tmp, path)?;

        info!("Config saved to {}", path.display());
        Ok(())
    }

    pub fn is_configured(&self) -> bool {
        !self.base_url.is_empty() && !self.username.is_empty() && !self.password.is_empty()
    }

    #[allow(dead_code)]
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

/// Atomic password-file writer: temp + rename. Mode is set to 0600 on
/// the temp file before rename so the secret is never world-readable.
pub fn write_password_file_atomic(
    path: &str,
    password: &str,
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
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&tmp)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        f.set_permissions(std::fs::Permissions::from_mode(0o600))?;
    }
    f.write_all(password.as_bytes())?;
    f.write_all(b"\n")?;
    f.sync_all()?;
    drop(f);
    std::fs::rename(&tmp, p)?;
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
        assert_eq!(config.password, "testpass");
    }

    #[test]
    fn test_is_configured() {
        let mut config = Config::new();
        assert!(!config.is_configured());

        config.base_url = "https://example.com".to_string();
        config.username = "user".to_string();
        config.password = "pass".to_string();
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
}
