use thiserror::Error;

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

    #[error("Daemon IPC error: {0}")]
    Ipc(#[from] crate::ipc::IpcError),
}

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

#[derive(Error, Debug)]
pub enum SubsonicError {
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),

    #[error("API error {code}: {message}")]
    Api { code: i32, message: String },

    #[allow(dead_code)]
    #[error("Authentication failed")]
    AuthFailed,

    #[allow(dead_code)]
    #[error("Server not configured")]
    NotConfigured,

    #[error("Failed to parse response: {0}")]
    Parse(String),

    #[error("URL parse error: {0}")]
    UrlParse(#[from] url::ParseError),
}

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

    #[allow(dead_code)]
    #[error("Queue is empty")]
    QueueEmpty,

    #[allow(dead_code)]
    #[error("Invalid queue index: {index}")]
    InvalidIndex { index: usize },

    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Error, Debug)]
pub enum UiError {
    #[error("Terminal initialization failed: {0}")]
    TerminalInit(std::io::Error),

    #[error("Render error: {0}")]
    Render(std::io::Error),

    #[error("Input error: {0}")]
    Input(std::io::Error),
}

#[allow(dead_code)]
pub type Result<T> = std::result::Result<T, Error>;
