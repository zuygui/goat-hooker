use std::path::PathBuf;

use crate::config::{impls::ConfigError, AppConfig};

#[derive(thiserror::Error, Debug)]
pub enum RunError {
    #[error(transparent)]
    Config(#[from] ConfigError),
}

pub fn run_hooks(p: PathBuf) -> Result<(), RunError> {
    let config = AppConfig::read(p).map_err(RunError::from)?;

    

    Ok(())
}
