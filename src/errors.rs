use std::path::PathBuf;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum GoatHookerError {
    #[error("Config file not found at {0:?}")]
    ConfigNotFound(PathBuf),

    #[error("Failed to read config file: {0}")]
    ReadError(#[from] std::io::Error),

    #[error("Failed to parse JSON config file {path:?}: {source}")]
    ParseError {
        path: PathBuf,
        source: serde_json::Error,
    },

    #[error("Failed to parse TOML config file {path:?}: {source}")]
    TomlError {
        path: PathBuf,
        source: Box<toml::de::Error>,
    },

    #[error("Failed to write the TOML config file {path:?}: {source}")]
    TomlWriteError {
        path: PathBuf,
        source: toml::ser::Error,
    },

    #[error("Invalid hook `{name}`: {reason}")]
    InvalidHook { name: String, reason: String },

    #[error("Invalid glob pattern `{pattern}` in hook `{hook}`: {source}")]
    InvalidPattern {
        hook: String,
        pattern: String,
        source: glob::PatternError,
    },

    #[error("Invalid import `{import}`: {source}")]
    InvalidImport {
        import: String,
        source: glob::PatternError,
    },

    #[error("Git error: {0}")]
    GitError(String),

    #[error("Hook `{0}` failed")]
    HookFailed(String),

    #[error("Hooks from other repositories aren't trusted yet: {}", .0.join(", "))]
    Untrusted(Vec<String>),

    #[error("{0}")]
    Network(String),

    #[error("Cancelled")]
    Cancelled,
}
