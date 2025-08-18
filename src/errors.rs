use thiserror::Error;

#[derive(Debug, Error)]
pub enum GoatHookerError {
    #[error("Config file not found")]
    ConfigNotFound,

    #[error("Failed to read config file: {0}")]
    ReadError(#[from] std::io::Error),

    #[error("Failed to parse JSON config file: {0}")]
    ParseError(#[from] serde_json::Error),
}
