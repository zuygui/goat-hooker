use std::{
    collections::HashSet,
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
        let content = fs::read_to_string(path).map_err(GoatHookerError::ReadError)?;
        let mut cfg: Config = serde_json::from_str(&content)
            .map_err(|_| GoatHookerError::ParseError(serde_json::Error::custom("Invalid JSON")))?;
        cfg.base_dir = Some(
            path.parent()
                .unwrap_or_else(|| Path::new("."))
                .to_path_buf(),
        );
        Ok(cfg)
    }
}

/// Loader qui gère la récursivité + détection des cycles
pub struct ConfigLoader {
    visited: HashSet<PathBuf>,
}

impl ConfigLoader {
    pub fn new() -> Self {
        Self {
            visited: HashSet::new(),
        }
    }

    pub fn load_recursive(&mut self, path: &Path) -> Result<Vec<Config>, GoatHookerError> {
        let abs_path = fs::canonicalize(path)?;
        if !abs_path.exists() {
            return Err(GoatHookerError::ReadError(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("Config file not found at {:?}", abs_path),
            )));
        }

        if !self.visited.insert(abs_path.clone()) {
            // déjà visité → on ignore pour éviter les cycles
            return Ok(vec![]);
        }

        let root = Config::load_from_file(&abs_path)?;
        let mut all_cfg = vec![root.clone()];

        if let Some(includes) = root.imports.clone() {
            for inc in includes {
                let sub_path = root
                    .base_dir
                    .as_ref()
                    .unwrap()
                    .join(&inc)
                    .join("hooker.config.json");

                if sub_path.exists() {
                    let sub_cfgs = self.load_recursive(&sub_path)?;
                    all_cfg.extend(sub_cfgs);
                } else {
                    eprintln!(
                        "⚠️ Warning: included path {:?} does not contain hooker.config.json",
                        sub_path
                    );
                }
            }
        }

        Ok(all_cfg)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    fn write_json(path: &Path, content: &str) {
        fs::write(path, content).expect("failed to write test json");
    }

    #[test]
    fn load_single_config() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join("hooker.config.json");

        write_json(
            &config_path,
            r#"{ "hooks": [ { "name": "test", "trigger": "pre-commit" } ] }"#,
        );

        let mut loader = ConfigLoader::new();
        let cfgs = loader.load_recursive(&config_path).unwrap();

        assert_eq!(cfgs.len(), 1);
        assert_eq!(cfgs[0].hooks.as_ref().unwrap()[0].name, "test");
    }

    #[test]
    fn load_with_imports() {
        let dir = tempdir().unwrap();

        // root config
        let root_path = dir.path().join("hooker.config.json");
        write_json(
            &root_path,
            r#"{ "imports": ["sub"], "hooks": [ { "name": "root-hook", "trigger": "pre-commit" } ] }"#,
        );

        // sub config
        let sub_dir = dir.path().join("sub");
        fs::create_dir(&sub_dir).unwrap();
        let sub_config_path = sub_dir.join("hooker.config.json");
        write_json(
            &sub_config_path,
            r#"{ "hooks": [ { "name": "sub-hook", "trigger": "pre-commit" } ] }"#,
        );

        let mut loader = ConfigLoader::new();
        let cfgs = loader.load_recursive(&root_path).unwrap();

        let hook_names: Vec<String> = cfgs
            .iter()
            .flat_map(|c| c.hooks.clone().unwrap_or_default())
            .map(|h| h.name)
            .collect();

        assert!(hook_names.contains(&"root-hook".to_string()));
        assert!(hook_names.contains(&"sub-hook".to_string()));
    }

    #[test]
    fn detect_cycles() {
        let dir = tempdir().unwrap();

        // root config imports sub
        let root_path = dir.path().join("hooker.config.json");
        write_json(
            &root_path,
            r#"{ "imports": ["sub"], "hooks": [ { "name": "root", "trigger": "pre-commit" } ] }"#,
        );

        // sub config imports root again (cycle)
        let sub_dir = dir.path().join("sub");
        fs::create_dir(&sub_dir).unwrap();
        let sub_path = sub_dir.join("hooker.config.json");
        write_json(
            &sub_path,
            &format!(
                r#"{{
                    "imports": [".."],
                    "hooks": [ {{ "name": "sub", "trigger": "pre-commit" }} ]
                }}"#
            ),
        );

        let mut loader = ConfigLoader::new();
        let cfgs = loader.load_recursive(&root_path).unwrap();

        // only 2 configs, no infinite recursion
        assert_eq!(cfgs.len(), 2);
    }
}
