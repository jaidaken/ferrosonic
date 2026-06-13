//! Crate-wide typed error hierarchy.

use thiserror::Error;

/// Top-level error uniting every subsystem's error type.
#[derive(Error, Debug)]
pub enum Error {
    /// Configuration loading, parsing, or validation failed.
    #[error("Configuration error: {0}")]
    Config(#[from] ConfigError),

    /// Subsonic server call failed.
    #[error("Subsonic API error: {0}")]
    Subsonic(#[from] SubsonicError),

    /// mpv or `PipeWire` interaction failed.
    #[error("Audio playback error: {0}")]
    Audio(#[from] AudioError),

    /// Terminal UI failure.
    #[error("UI error: {0}")]
    Ui(#[from] UiError),

    /// Underlying I/O failure.
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Daemon socket communication failed.
    #[error("Daemon IPC error: {0}")]
    Ipc(#[from] crate::ipc::IpcError),
}

/// Errors from loading, parsing, or writing the config file.
#[derive(Error, Debug)]
pub enum ConfigError {
    /// No config file exists at the expected path.
    #[error("Config file not found at {path}")]
    NotFound {
        /// Path that was checked.
        path: String,
    },

    /// Config file is not valid TOML.
    #[error("Failed to parse config: {0}")]
    Parse(#[from] toml::de::Error),

    /// Config could not be encoded back to TOML.
    #[error("Failed to serialize config: {0}")]
    Serialize(#[from] toml::ser::Error),

    /// A required config field is absent.
    #[error("Missing required field: {field}")]
    MissingField {
        /// Name of the absent field.
        field: String,
    },

    /// A config field holds a malformed URL.
    #[error("Invalid URL: {url}")]
    InvalidUrl {
        /// The malformed URL value.
        url: String,
    },

    /// Reading or writing the config file failed.
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Errors from talking to the Subsonic server.
#[derive(Error, Debug)]
pub enum SubsonicError {
    /// Transport-level HTTP failure.
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),

    /// Server answered with a Subsonic error object.
    #[error("API error {code}: {message}")]
    Api {
        /// Subsonic error code.
        code: i32,
        /// Server-supplied error message.
        message: String,
    },

    /// Credentials were rejected.
    #[error("Authentication failed")]
    AuthFailed,

    /// No server credentials are configured yet.
    #[error("Server not configured")]
    NotConfigured,

    /// Response body did not match the expected shape.
    #[error("Failed to parse response: {0}")]
    Parse(String),

    /// Base URL or request URL failed to parse.
    #[error("URL parse error: {0}")]
    UrlParse(#[from] url::ParseError),
}

/// Errors from mpv and `PipeWire` control.
#[derive(Error, Debug)]
pub enum AudioError {
    /// Command issued while no mpv process is alive.
    #[error("MPV not running")]
    MpvNotRunning,

    /// mpv process failed to start.
    #[error("Failed to spawn MPV: {0}")]
    MpvSpawn(std::io::Error),

    /// mpv answered an IPC command with an error.
    #[error("MPV IPC error: {0}")]
    MpvIpc(String),

    /// Connecting to mpv's IPC socket failed.
    #[error("MPV socket connection failed: {0}")]
    MpvSocket(std::io::Error),

    /// pw-metadata invocation failed.
    #[error("PipeWire command failed: {0}")]
    PipeWire(String),

    /// Operation requires a non-empty queue.
    #[error("Queue is empty")]
    QueueEmpty,

    /// Queue index is out of bounds.
    #[error("Invalid queue index: {index}")]
    InvalidIndex {
        /// The out-of-bounds index.
        index: usize,
    },

    /// mpv IPC JSON encode or decode failed.
    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),

    /// Underlying I/O failure.
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Errors from the terminal UI layer.
#[derive(Error, Debug)]
pub enum UiError {
    /// Entering raw mode or the alternate screen failed.
    #[error("Terminal initialization failed: {0}")]
    TerminalInit(std::io::Error),

    /// Drawing a frame failed.
    #[error("Render error: {0}")]
    Render(std::io::Error),

    /// Reading terminal input failed.
    #[error("Input error: {0}")]
    Input(std::io::Error),
}

/// Crate-wide result alias over [`Error`].
pub type Result<T> = std::result::Result<T, Error>;
