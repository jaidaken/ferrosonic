//! Persisted TOML configuration: load/save, password resolution, repeat mode.

/// Well-known config and data directory paths.
pub mod paths;

use serde::{Deserialize, Serialize};
use std::path::Path;
use tracing::{debug, info, warn};

use crate::error::ConfigError;
use crate::io_util::{atomic_write_bytes_private, fsync_parent_dir};
use crate::secret::{serialize_revealed, Secret};

/// All top-level TOML keys we expect. Anything not in this list is
/// warned on load so a typo like `RepeateMode` is visible instead of
/// silently reverting to the default.
pub const KNOWN_CONFIG_KEYS: &[&str] = &[
    "BaseURL",
    "Username",
    "Password",
    "PasswordFile",
    "PasswordEval",
    "PasswordKeyring",
    "Theme",
    "Cava",
    "CavaSize",
    "Daemon",
    "AutoContinue",
    "RepeatMode",
    "CoverArt",
    "CoverArtSize",
    "Scrobble",
    "Notifications",
    "RateSwitchDelayMs",
    "MusicFolderId",
    "MusicFolderChosen",
];

/// A command run to obtain the password: a shell string or an argv array.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum PasswordEval {
    /// Run via `sh -c`; the shell expands env vars, `~`, and pipes.
    Shell(String),
    /// Direct exec of `[program, args...]`; no shell involved.
    Argv(Vec<String>),
}

/// User configuration, persisted as TOML at the path from [`paths::config_file`].
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Config {
    /// Subsonic server base URL, scheme included.
    #[serde(rename = "BaseURL", default)]
    pub base_url: String,

    /// Subsonic account username.
    #[serde(rename = "Username", default)]
    pub username: String,

    /// Resolved at load-time from env, `PasswordEval`, `PasswordFile`, then this inline value. Secret masks Debug + Serialize so accidental log/wire paths emit "***"; `save_to_file` routes through `ConfigOnDisk` which writes the real value.
    #[serde(rename = "Password", default)]
    pub password: Secret,

    /// Path of a file holding the password; takes priority over the inline value.
    #[serde(
        rename = "PasswordFile",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub password_file: Option<String>,

    /// Command whose stdout is the password; takes priority over `PasswordFile`
    /// and the inline value, but not the `FERROSONIC_PASSWORD` env var. Must be
    /// non-interactive: the daemon runs it without a terminal.
    #[serde(
        rename = "PasswordEval",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub password_eval: Option<PasswordEval>,

    /// True when the password lives in the OS keychain, keyed by `base_url` +
    /// `username`. Resolved after `PasswordFile` and before the inline value.
    /// When set, no plaintext password is written to the config file.
    #[serde(rename = "PasswordKeyring", default)]
    pub password_keyring: bool,

    /// Active theme name; empty selects the built-in default.
    #[serde(rename = "Theme", default)]
    pub theme: String,

    /// Whether the cava visualizer is enabled.
    #[serde(rename = "Cava", default)]
    pub cava: bool,

    /// Cava visualizer height in rows.
    #[serde(rename = "CavaSize", default = "Config::default_cava_size")]
    pub cava_size: u8,

    /// `false` forces standalone mode on next launch.
    #[serde(rename = "Daemon", default = "Config::default_daemon")]
    pub daemon: bool,

    /// Auto-continue with random songs when the queue ends.
    #[serde(rename = "AutoContinue", default)]
    pub auto_continue: bool,

    /// Queue repeat mode.
    #[serde(rename = "RepeatMode", default)]
    pub repeat_mode: RepeatMode,

    /// Whether cover art rendering is enabled.
    #[serde(rename = "CoverArt", default)]
    pub cover_art: bool,

    /// Total height of the now-playing section in rows when cover art
    /// is visible. Range 8..=24, step 2. The art height is this minus
    /// 3 (2 border rows + 1 progress bar row).
    #[serde(rename = "CoverArtSize", default = "Config::default_cover_art_size")]
    pub cover_art_size: u8,

    /// Report plays to the server (scrobble / playbackReport). On by default.
    #[serde(rename = "Scrobble", default = "Config::default_scrobble")]
    pub scrobble: bool,

    /// Show a desktop notification on track change (Linux D-Bus). On by default.
    #[serde(rename = "Notifications", default = "Config::default_notifications")]
    pub notifications: bool,

    /// Milliseconds to hold the track paused after re-clocking the audio
    /// device so the `PipeWire` rate switch lands in silence, not in the
    /// first frames of music. Device-dependent; raise for DACs that
    /// re-lock slowly. Only applied when the rate actually changes.
    #[serde(
        rename = "RateSwitchDelayMs",
        default = "Config::default_rate_switch_delay_ms"
    )]
    pub rate_switch_delay_ms: u32,

    /// Library to browse and play from (`musicFolderId`); `None` = all.
    #[serde(rename = "MusicFolderId", default)]
    pub music_folder_id: Option<i64>,

    /// True once the user has picked a library; until then the daemon defaults
    /// to the server's first (default) library rather than all libraries.
    #[serde(rename = "MusicFolderChosen", default)]
    pub music_folder_chosen: bool,
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
    #[serde(rename = "PasswordEval", skip_serializing_if = "Option::is_none")]
    password_eval: Option<&'a PasswordEval>,
    #[serde(rename = "PasswordKeyring", skip_serializing_if = "std::ops::Not::not")]
    password_keyring: bool,
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
    #[serde(rename = "Scrobble")]
    scrobble: bool,
    #[serde(rename = "Notifications")]
    notifications: bool,
    #[serde(rename = "RateSwitchDelayMs")]
    rate_switch_delay_ms: u32,
    #[serde(rename = "MusicFolderId", skip_serializing_if = "Option::is_none")]
    music_folder_id: Option<i64>,
    #[serde(
        rename = "MusicFolderChosen",
        skip_serializing_if = "std::ops::Not::not"
    )]
    music_folder_chosen: bool,
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
        let pw_file_set = self.password_file.as_ref().is_some_and(|s| !s.is_empty());
        // The secret lives outside the file when a PasswordFile, PasswordEval,
        // or the OS keychain holds it; do not write the plaintext back inline.
        let secret_external = pw_file_set || self.password_eval.is_some() || self.password_keyring;
        ConfigOnDisk {
            base_url: &self.base_url,
            username: &self.username,
            password: if secret_external || self.password.is_empty() {
                None
            } else {
                Some(&self.password)
            },
            password_file: self.password_file.as_deref(),
            password_eval: self.password_eval.as_ref(),
            password_keyring: self.password_keyring,
            theme: &self.theme,
            cava: self.cava,
            cava_size: self.cava_size,
            daemon: self.daemon,
            auto_continue: self.auto_continue,
            repeat_mode: self.repeat_mode,
            cover_art: self.cover_art,
            cover_art_size: self.cover_art_size,
            scrobble: self.scrobble,
            notifications: self.notifications,
            rate_switch_delay_ms: self.rate_switch_delay_ms,
            music_folder_id: self.music_folder_id,
            music_folder_chosen: self.music_folder_chosen,
        }
    }
}

/// Queue repeat behavior.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum RepeatMode {
    /// Stop at the end of the queue.
    #[default]
    Off,
    /// Repeat the current track.
    One,
    /// Wrap to the start at the end of the queue.
    All,
}

impl RepeatMode {
    /// Lowercase label shown in the footer.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::One => "one",
            Self::All => "all",
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
    #[must_use]
    pub const fn cycle(self) -> Self {
        match self {
            Self::Off => Self::One,
            Self::One => Self::All,
            Self::All => Self::Off,
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
    #[must_use]
    pub const fn next_auto(self, current: usize, queue_len: usize) -> Option<usize> {
        if queue_len == 0 {
            return None;
        }
        match self {
            Self::One => Some(current),
            Self::All => Some((current + 1) % queue_len),
            Self::Off => {
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
    #[must_use]
    pub const fn next_manual(self, current: usize, queue_len: usize) -> Option<usize> {
        if queue_len == 0 {
            return None;
        }
        match self {
            Self::All | Self::One => Some((current + 1) % queue_len),
            Self::Off => {
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
    #[must_use]
    pub const fn prev_wrap(self, queue_len: usize) -> Option<usize> {
        if queue_len == 0 {
            return None;
        }
        match self {
            Self::All | Self::One => Some(queue_len - 1),
            Self::Off => None,
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
            scrobble: Self::default_scrobble(),
            notifications: Self::default_notifications(),
            rate_switch_delay_ms: Self::default_rate_switch_delay_ms(),
            music_folder_id: None,
            music_folder_chosen: false,
            password_eval: None,
            password_keyring: false,
        }
    }
}

impl Config {
    const fn default_cava_size() -> u8 {
        40
    }

    const fn default_daemon() -> bool {
        true
    }

    const fn default_cover_art_size() -> u8 {
        16
    }

    const fn default_scrobble() -> bool {
        true
    }

    const fn default_notifications() -> bool {
        true
    }

    const fn default_rate_switch_delay_ms() -> u32 {
        500
    }

    /// Alias for [`Config::default`].
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Load from the default config path, falling back to defaults when absent.
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

    /// Resolves the password in priority order: `FERROSONIC_PASSWORD` env > `PasswordEval` > `PasswordFile` > OS keychain > inline.
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
        let mut config: Self = toml::from_str(&contents)?;
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
    #[must_use]
    pub fn expand_tilde(path: &str) -> String {
        if let Some(rest) = path.strip_prefix("~/") {
            if let Ok(home) = std::env::var("HOME") {
                return format!("{home}/{rest}");
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
        if let Some(eval) = self.password_eval.as_ref() {
            match run_password_eval(eval) {
                Ok(secret) => {
                    debug!("Using password from PasswordEval");
                    self.password = Secret::from_string(secret);
                }
                Err(e) => {
                    warn!("{e}; clearing inline password to avoid a stale credential");
                    self.password.clear();
                }
            }
            return;
        }
        if let Some(pf) = self.password_file.as_ref().filter(|s| !s.is_empty()) {
            let expanded = Self::expand_tilde(pf);
            match std::fs::read_to_string(&expanded) {
                Ok(mut contents) => {
                    debug!("Using password from {}", expanded);
                    let secret = extract_secret_line(&contents);
                    use zeroize::Zeroize;
                    contents.zeroize();
                    self.password = Secret::from_string(secret);
                }
                Err(e) => {
                    warn!(
                        "PasswordFile {} unreadable ({}); clearing inline password to avoid silent fallback to stale credentials",
                        expanded, e
                    );
                    self.password.clear();
                }
            }
            return;
        }
        if self.password_keyring {
            match crate::secret_store::retrieve(&self.base_url, &self.username) {
                Ok(Some(secret)) => {
                    debug!("Using password from the OS keychain");
                    self.password = secret;
                }
                Ok(None) => {
                    warn!("PasswordKeyring set but no entry in the OS keychain; clearing inline password to avoid a stale credential");
                    self.password.clear();
                }
                Err(e) => {
                    warn!("{e}; clearing inline password to avoid a stale credential");
                    self.password.clear();
                }
            }
        }
    }

    /// Save to the default config path.
    pub fn save_default(&self) -> Result<(), ConfigError> {
        let path = paths::config_file().ok_or_else(|| ConfigError::NotFound {
            path: "default config location".to_string(),
        })?;

        self.save_to_file(&path)
    }

    /// Atomically write the config TOML; round-trips via `load_from_file`.
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
        // Owner-only: the file may hold an inline plaintext password.
        atomic_write_bytes_private(path, contents.as_bytes())?;
        info!("Config saved to {}", path.display());
        Ok(())
    }

    /// True when `base_url`, username, and password are all non-empty.
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
    #[must_use]
    pub fn is_configured(&self) -> bool {
        !self.base_url.is_empty() && !self.username.is_empty() && !self.password.is_empty()
    }

    /// The resolved password in plain text.
    #[must_use]
    pub fn password_str(&self) -> &str {
        self.password.reveal()
    }

    /// Reject empty or malformed `base_url`. Empty username/password warn only.
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
/// The secret carried by a password source: the first line, minus a trailing
/// `\r`. Tolerates `pass`/`secret-tool` style output that appends metadata or a
/// newline; keeps the password verbatim otherwise (including trailing spaces).
fn extract_secret_line(raw: &str) -> String {
    raw.split('\n')
        .next()
        .unwrap_or("")
        .trim_end_matches('\r')
        .to_string()
}

/// Expand a leading `~/` and `$VAR` / `${VAR}` references for an argv argument.
/// The shell form does this itself; the argv form has no shell, so we do it.
fn expand_env_tilde(arg: &str) -> String {
    let expanded = Config::expand_tilde(arg);
    let bytes = expanded.as_bytes();
    let mut out = String::with_capacity(expanded.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'$' && i + 1 < bytes.len() {
            let (name, next) = if bytes[i + 1] == b'{' {
                let end = expanded[i + 2..].find('}').map(|p| i + 2 + p);
                match end {
                    Some(e) => (&expanded[i + 2..e], e + 1),
                    None => (&expanded[(i + 1)..=i], i + 1),
                }
            } else {
                let mut e = i + 1;
                while e < bytes.len() && (bytes[e].is_ascii_alphanumeric() || bytes[e] == b'_') {
                    e += 1;
                }
                (&expanded[i + 1..e], e)
            };
            if name.is_empty() {
                out.push('$');
                i += 1;
            } else {
                out.push_str(&std::env::var(name).unwrap_or_default());
                i = next;
            }
        } else {
            out.push(expanded[i..].chars().next().unwrap_or('\0'));
            i += expanded[i..].chars().next().map_or(1, char::len_utf8);
        }
    }
    out
}

/// Run a `PasswordEval` command and return its secret. Headless-safe: no stdin,
/// own session (`setsid`), a 30s timeout, and a process-group kill on timeout so
/// a hung child (e.g. a `pinentry` with no terminal) cannot stall startup.
fn run_password_eval(eval: &PasswordEval) -> Result<String, String> {
    run_password_eval_timeout(eval, std::time::Duration::from_secs(30))
}

fn run_password_eval_timeout(
    eval: &PasswordEval,
    timeout: std::time::Duration,
) -> Result<String, String> {
    use std::io::Read;
    use std::os::unix::process::CommandExt;
    use std::process::{Command, Stdio};
    use zeroize::Zeroize;

    let mut cmd = match eval {
        PasswordEval::Shell(s) => {
            let mut c = Command::new("sh");
            c.arg("-c").arg(s);
            c
        }
        PasswordEval::Argv(parts) => {
            let Some((prog, args)) = parts.split_first() else {
                return Err("PasswordEval array is empty".to_string());
            };
            let mut c = Command::new(expand_env_tilde(prog));
            for a in args {
                c.arg(expand_env_tilde(a));
            }
            c
        }
    };
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    // SAFETY: setsid is async-signal-safe; new session enables a group kill.
    unsafe {
        cmd.pre_exec(|| match libc::setsid() {
            -1 => Err(std::io::Error::last_os_error()),
            _ => Ok(()),
        });
    }

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("PasswordEval failed to start: {e}"))?;
    let pid = child.id() as libc::pid_t;
    let mut stdout = child.stdout.take().ok_or("PasswordEval: no stdout pipe")?;
    let mut stderr = child.stderr.take().ok_or("PasswordEval: no stderr pipe")?;

    // Drain both pipes in threads so a child writing past the pipe buffer cannot
    // deadlock against the timeout wait.
    let out_h = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = stdout.read_to_end(&mut buf);
        buf
    });
    let err_h = std::thread::spawn(move || {
        let mut buf = String::new();
        let _ = stderr.read_to_string(&mut buf);
        buf
    });

    // This thread is the sole owner and reaper of `child`, so a timeout kill
    // always targets the still-live child's process group (no reused-PID race).
    let deadline = std::time::Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break s,
            Ok(None) => {
                if std::time::Instant::now() >= deadline {
                    // SAFETY: child not yet reaped; pid is its group leader.
                    unsafe {
                        libc::kill(-pid, libc::SIGKILL);
                    }
                    let _ = child.wait();
                    return Err("PasswordEval timed out".to_string());
                }
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            Err(e) => return Err(format!("PasswordEval wait failed: {e}")),
        }
    };

    let mut out_bytes = out_h.join().unwrap_or_default();
    let stderr_text = err_h.join().unwrap_or_default();
    if !status.success() {
        out_bytes.zeroize();
        let code = status
            .code()
            .map_or_else(|| "signal".to_string(), |c| c.to_string());
        let detail = stderr_text.trim();
        return Err(format!("PasswordEval exited {code}: {detail}"));
    }
    let mut raw = String::from_utf8_lossy(&out_bytes).into_owned();
    out_bytes.zeroize();
    let secret = extract_secret_line(&raw);
    raw.zeroize();
    if secret.is_empty() {
        return Err("PasswordEval produced no output".to_string());
    }
    Ok(secret)
}

/// Write `password` to the `PasswordFile` at `path` (tilde-expanded), owner-only
/// (`0o600`), via temp + rename so a concurrent read never sees a partial file.
///
/// # Errors
/// Returns an [`std::io::Error`] if the directory, write, or rename fails.
pub fn write_password_file_atomic(path: &str, password: &Secret) -> std::io::Result<()> {
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
    fn every_serialized_config_key_is_known() {
        let mut c = Config::default();
        c.base_url = "https://x".into();
        c.username = "u".into();
        c.password = "p".into();
        let toml = toml::to_string(&c.as_on_disk()).expect("serialize config");
        for line in toml.lines() {
            if let Some(key) = line.split('=').next().map(str::trim) {
                if key.is_empty() {
                    continue;
                }
                assert!(
                    KNOWN_CONFIG_KEYS.contains(&key),
                    "config key {key:?} is serialized but missing from KNOWN_CONFIG_KEYS; \
                     add it or it warns as unknown on load"
                );
            }
        }
    }

    #[test]
    fn extract_secret_line_takes_first_line_keeps_trailing_space() {
        assert_eq!(extract_secret_line("pw\n"), "pw");
        assert_eq!(extract_secret_line("pw\r\n"), "pw");
        assert_eq!(extract_secret_line("pw\nmeta\nmore"), "pw");
        assert_eq!(extract_secret_line("pw with space "), "pw with space ");
        assert_eq!(extract_secret_line(""), "");
    }

    #[test]
    fn expand_env_tilde_expands_vars() {
        std::env::set_var("FERRO_TEST_X", "hunter2");
        assert_eq!(expand_env_tilde("$FERRO_TEST_X"), "hunter2");
        assert_eq!(expand_env_tilde("${FERRO_TEST_X}-x"), "hunter2-x");
        assert_eq!(expand_env_tilde("literal"), "literal");
    }

    #[test]
    fn password_eval_shell_form_captures_first_line() {
        let r = run_password_eval(&PasswordEval::Shell("printf 'navipass\\nmeta'".into()));
        assert_eq!(r, Ok("navipass".to_string()));
    }

    #[test]
    fn password_eval_argv_form_expands_env() {
        std::env::set_var("FERRO_TEST_PW", "argvpass");
        let r = run_password_eval(&PasswordEval::Argv(vec![
            "printf".into(),
            "%s".into(),
            "$FERRO_TEST_PW".into(),
        ]));
        assert_eq!(r, Ok("argvpass".to_string()));
    }

    #[test]
    fn password_eval_nonzero_exit_and_empty_output_fail() {
        assert!(run_password_eval(&PasswordEval::Shell("exit 3".into())).is_err());
        assert!(run_password_eval(&PasswordEval::Shell("true".into())).is_err());
    }

    #[test]
    fn password_eval_times_out_without_waiting_for_the_child() {
        let start = std::time::Instant::now();
        let r = run_password_eval_timeout(
            &PasswordEval::Shell("sleep 5".into()),
            std::time::Duration::from_millis(200),
        );
        assert!(r.is_err(), "a hung command must time out");
        assert!(
            start.elapsed() < std::time::Duration::from_secs(3),
            "the timeout must not block on the child"
        );
    }

    #[test]
    fn password_eval_resolves_at_config_load() {
        let mut f = NamedTempFile::new().unwrap();
        write!(
            f,
            "BaseURL=\"https://x\"\nUsername=\"u\"\nPasswordEval=\"printf loadpass\"\n"
        )
        .unwrap();
        let c = Config::load_from_file(f.path()).unwrap();
        assert_eq!(c.password_str(), "loadpass");
    }

    #[test]
    fn save_preserves_password_eval_and_omits_inline_password() {
        let mut c = Config::default();
        c.base_url = "https://x".into();
        c.password = "resolved-secret".into();
        c.password_eval = Some(PasswordEval::Shell("printf x".into()));
        let f = NamedTempFile::new().unwrap();
        c.save_to_file(f.path()).unwrap();
        let written = std::fs::read_to_string(f.path()).unwrap();
        assert!(
            written.contains("PasswordEval"),
            "PasswordEval preserved:\n{written}"
        );
        assert!(
            !written.contains("resolved-secret"),
            "the resolved plaintext must not be written back inline:\n{written}"
        );
    }

    #[test]
    fn save_with_keyring_marker_omits_inline_password() {
        let mut c = Config::default();
        c.base_url = "https://x".into();
        c.username = "u".into();
        c.password = "resolved-secret".into();
        c.password_keyring = true;
        let f = NamedTempFile::new().unwrap();
        c.save_to_file(f.path()).unwrap();
        let written = std::fs::read_to_string(f.path()).unwrap();
        assert!(
            written.contains("PasswordKeyring = true"),
            "keyring marker preserved:\n{written}"
        );
        assert!(
            !written.contains("resolved-secret"),
            "the resolved plaintext must not be written inline when keyring holds it:\n{written}"
        );
    }

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
