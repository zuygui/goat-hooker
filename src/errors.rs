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

    #[error("Cancelled")]
    Cancelled,
}
