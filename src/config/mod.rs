use std::{
    fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, de::Error};

use crate::errors::GoatHookerError;

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub imports: Option<Vec<String>>,
    pub hooks: Option<Vec<Hook>>,
    #[serde(skip)]
    pub base_dir: Option<PathBuf>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Hook {
    pub name: String,
    pub trigger: Trigger,
    pub include: Option<Vec<String>>,
    pub exclude: Option<Vec<String>>,
    pub cmd: Option<String>,
    pub cmds: Option<Vec<String>>,
    pub from: Option<String>,
    pub path: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub enum Trigger {
    #[serde(rename = "pre-commit")]
    PreCommit,
}

impl Config {
    pub fn load_from_file(path: &Path) -> Result<Config, GoatHookerError> {
        let content = fs::read_to_string(path).map_err(|e| GoatHookerError::ReadError(e))?;
        let mut cfg: Config = serde_json::from_str(&content)
            .map_err(|_| GoatHookerError::ParseError(serde_json::Error::custom("Invalid JSON")))?;
        cfg.base_dir = Some(
            path.parent()
                .unwrap_or_else(|| Path::new("."))
                .to_path_buf(),
        );
        Ok(cfg)
    }

    pub fn load_recursive(path: &Path) -> Result<Vec<Config>, GoatHookerError> {
        let root = Config::load_from_file(path)?;
        let mut all_cfg = vec![root.clone()];

        if let Some(includes) = root.imports.clone() {
            for inc in includes {
                let sub_path = root
                    .base_dir
                    .as_ref()
                    .unwrap()
                    .join(inc)
                    .join("hooker.config.json");

                if sub_path.exists() {
                    let sub_cfgs = Config::load_recursive(&sub_path)?;
                    all_cfg.extend(sub_cfgs);
                } else {
                    eprintln!(
                        "⚠️ Warning: included path {:?} does not contain goat-hooker.json",
                        sub_path
                    );
                }
            }
        }

        Ok(all_cfg)
    }
}
