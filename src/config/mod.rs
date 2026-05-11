//! Configuration loading and management

pub mod paths;

use serde::{Deserialize, Serialize};
use std::path::Path;
use tracing::{debug, info, warn};

use crate::error::ConfigError;

/// Main application configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Subsonic server base URL
    #[serde(rename = "BaseURL", default)]
    pub base_url: String,

    /// Username for authentication
    #[serde(rename = "Username", default)]
    pub username: String,

    /// Password for authentication. Plaintext fallback. Prefer `PasswordFile`
    /// or the `FERROSONIC_PASSWORD` environment variable to keep secrets out
    /// of the on-disk config.
    #[serde(rename = "Password", default)]
    pub password: String,

    /// Path to a file containing the password (one line, trailing whitespace
    /// trimmed). Useful with `pass`, `gopass`, or any secret manager that can
    /// write to a path. Lower priority than `FERROSONIC_PASSWORD` env var,
    /// higher priority than inline `Password`.
    #[serde(rename = "PasswordFile", default, skip_serializing_if = "Option::is_none")]
    pub password_file: Option<String>,

    /// UI Theme name
    #[serde(rename = "Theme", default)]
    pub theme: String,

    /// Enable cava audio visualizer
    #[serde(rename = "Cava", default)]
    pub cava: bool,

    /// Cava visualizer height percentage (10-80, step 5)
    #[serde(rename = "CavaSize", default = "Config::default_cava_size")]
    pub cava_size: u8,

    /// Enable the ferrosonicd daemon. When `true` (default) the TUI
    /// connects to a running daemon and auto-spawns one if missing.
    /// When `false` the TUI always runs in standalone (in-process)
    /// mode and music stops when the terminal closes. The `--standalone`
    /// CLI flag overrides this to `false` for a one-off launch.
    #[serde(rename = "Daemon", default = "Config::default_daemon")]
    pub daemon: bool,

    /// When the queue runs out, fetch a fresh batch of random songs
    /// from the server and keep playing. Default `false`.
    #[serde(rename = "AutoContinue", default)]
    pub auto_continue: bool,

    /// Repeat mode for the queue. `Off` plays through and stops,
    /// `One` loops the current track, `All` wraps to the queue head.
    /// Persists across daemon restarts.
    #[serde(rename = "RepeatMode", default)]
    pub repeat_mode: RepeatMode,

    /// Show album art in the cava band on the now-playing screen.
    /// Requires a terminal with image support (kitty / iTerm2 / sixel)
    /// for full fidelity; half-block fallback otherwise.
    #[serde(rename = "CoverArt", default)]
    pub cover_art: bool,
}

/// How the queue behaves when it reaches the end.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum RepeatMode {
    /// Stop when the queue ends (or fire auto-continue if enabled).
    #[default]
    Off,
    /// Loop the currently-playing track forever.
    One,
    /// Wrap the queue position back to 0 when the end is reached.
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
    /// What track plays after `current` finishes on its own.
    /// `One` repeats it; `All` wraps; `Off` advances and gives up
    /// at the end (caller handles auto-continue / stop).
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
    /// What track plays when the user explicitly presses Next.
    /// `One` is ignored on manual skip — the user wanted to move,
    /// so wrap or stop as if Off / All.
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
    /// What track plays when the user explicitly presses Previous from
    /// position 0. `All` / `One` wrap to the last track; `Off` stays.
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

    /// Create a new empty config
    pub fn new() -> Self {
        Self::default()
    }

    /// Load config from the default location
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

    /// Load config from a specific file. Resolves the password from (in order
    /// of priority): `FERROSONIC_PASSWORD` env var > `PasswordFile` > inline
    /// `Password`. Higher-priority sources overwrite lower ones in-place so
    /// downstream code can keep using `config.password`.
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

    /// Resolve the effective password. Checks sources in priority order and
    /// overwrites `self.password` with the first one that yields a value.
    fn resolve_password(&mut self) {
        if let Ok(env) = std::env::var("FERROSONIC_PASSWORD") {
            if !env.is_empty() {
                debug!("Using password from FERROSONIC_PASSWORD env var");
                self.password = env;
                return;
            }
        }
        if let Some(pf) = self.password_file.as_ref().filter(|s| !s.is_empty()) {
            // Expand a leading ~ for convenience
            let expanded = if let Some(rest) = pf.strip_prefix("~/") {
                if let Ok(home) = std::env::var("HOME") {
                    format!("{}/{}", home, rest)
                } else {
                    pf.clone()
                }
            } else {
                pf.clone()
            };
            match std::fs::read_to_string(&expanded) {
                Ok(contents) => {
                    debug!("Using password from {}", expanded);
                    self.password = contents.trim_end_matches(['\n', '\r', ' ', '\t']).to_string();
                    return;
                }
                Err(e) => {
                    warn!("PasswordFile {} unreadable: {}; falling back to inline password", expanded, e);
                }
            }
        }
        // else: keep the inline self.password as-is
    }

    /// Save config to the default location
    pub fn save_default(&self) -> Result<(), ConfigError> {
        let path = paths::config_file().ok_or_else(|| ConfigError::NotFound {
            path: "default config location".to_string(),
        })?;

        self.save_to_file(&path)
    }

    /// Save config to a specific file
    pub fn save_to_file(&self, path: &Path) -> Result<(), ConfigError> {
        debug!("Saving config to {}", path.display());

        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            if !parent.exists() {
                std::fs::create_dir_all(parent)?;
            }
        }

        let contents = toml::to_string_pretty(self)?;
        std::fs::write(path, contents)?;

        info!("Config saved to {}", path.display());
        Ok(())
    }

    /// Check if the config has valid server settings
    pub fn is_configured(&self) -> bool {
        !self.base_url.is_empty() && !self.username.is_empty() && !self.password.is_empty()
    }

    /// Validate the config
    #[allow(dead_code)]
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.base_url.is_empty() {
            return Err(ConfigError::MissingField {
                field: "BaseURL".to_string(),
            });
        }

        // Validate URL format
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
}
