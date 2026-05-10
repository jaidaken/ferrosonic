//! Error types for ferrosonic

use thiserror::Error;

/// Main error type for ferrosonic
#[derive(Error, Debug)]
pub enum Error {
    #[error("Configuration error: {0}")]
    Config(#[from] ConfigError),

    #[error("Subsonic API error: {0}")]
    Subsonic(#[from] SubsonicError),

    #[error("Audio playback error: {0}")]
    Audio(#[from] AudioError),

    #[error("UI error: {0}")]
    Ui(#[from] UiError),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Configuration-related errors
#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("Config file not found at {path}")]
    NotFound { path: String },

    #[error("Failed to parse config: {0}")]
    Parse(#[from] toml::de::Error),

    #[error("Failed to serialize config: {0}")]
    Serialize(#[from] toml::ser::Error),

    #[error("Missing required field: {field}")]
    MissingField { field: String },

    #[error("Invalid URL: {url}")]
    InvalidUrl { url: String },

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Subsonic API errors
#[derive(Error, Debug)]
pub enum SubsonicError {
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),

    #[error("API error {code}: {message}")]
    Api { code: i32, message: String },

    #[allow(dead_code)] // reserved for future auth-failure surface
    #[error("Authentication failed")]
    AuthFailed,

    #[allow(dead_code)] // reserved for future "no server configured" surface
    #[error("Server not configured")]
    NotConfigured,

    #[error("Failed to parse response: {0}")]
    Parse(String),

    #[error("URL parse error: {0}")]
    UrlParse(#[from] url::ParseError),
}

/// Audio playback errors
#[derive(Error, Debug)]
pub enum AudioError {
    #[error("MPV not running")]
    MpvNotRunning,

    #[error("Failed to spawn MPV: {0}")]
    MpvSpawn(std::io::Error),

    #[error("MPV IPC error: {0}")]
    MpvIpc(String),

    #[error("MPV socket connection failed: {0}")]
    MpvSocket(std::io::Error),

    #[error("PipeWire command failed: {0}")]
    PipeWire(String),

    #[allow(dead_code)] // reserved for queue-operation error paths
    #[error("Queue is empty")]
    QueueEmpty,

    #[allow(dead_code)] // reserved for queue-operation error paths
    #[error("Invalid queue index: {index}")]
    InvalidIndex { index: usize },

    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// UI-related errors
#[derive(Error, Debug)]
pub enum UiError {
    #[error("Terminal initialization failed: {0}")]
    TerminalInit(std::io::Error),

    #[error("Render error: {0}")]
    Render(std::io::Error),

    #[error("Input error: {0}")]
    Input(std::io::Error),
}

/// Result type alias using our Error
#[allow(dead_code)] // exported for downstream use; not yet referenced internally
pub type Result<T> = std::result::Result<T, Error>;
