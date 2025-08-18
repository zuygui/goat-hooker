use std::{fs, path::Path};

use crate::{config::Config, errors::GoatHookerError};

pub fn load_cfg() -> Result<Config, GoatHookerError> {
    let candidates = vec![
        "hooker.config.json",
        "hooker.json",
        "goat-hooker.config.json",
        "goat-hooker.json",
    ];

    let file_path = candidates
        .into_iter()
        .find(|p| Path::new(p).exists())
        .ok_or(GoatHookerError::ConfigNotFound)?;

    let content = fs::read_to_string(file_path)?;
    let cfg: Config =
        serde_json::from_str(&content).map_err(|_| GoatHookerError::ConfigNotFound)?;

    Ok(cfg)
}
