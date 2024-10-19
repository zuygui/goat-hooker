use serde_yml;
use std::{collections::HashMap, fs, path::Path};

use super::{AppConfig, HOOK_CONFIG_FILENAME};

#[derive(thiserror::Error, Debug)]
pub enum ConfigError {
    #[error("failed to read config file")]
    IO(#[from] std::io::Error),
    #[error("failed to parse config file")]
    Yaml(#[from] serde_yml::Error),
}

impl Default for AppConfig {
    fn default() -> Self {
        AppConfig { hooks: HashMap::new() }
    }
}

impl AppConfig {
    pub fn write_config(self, path: impl AsRef<Path>) -> Result<(), ConfigError> {
        // Save the configuration
        let str_config = serde_yml::to_string(&self).unwrap();
        fs::write(path.as_ref().join(HOOK_CONFIG_FILENAME), str_config).map_err(ConfigError::from)
    }

    pub fn exists() -> bool {
        Path::new(HOOK_CONFIG_FILENAME).exists()
    }

    pub fn read(path: impl AsRef<Path>) -> Result<AppConfig, ConfigError> {
        let file_path = path.as_ref();
        let content = fs::read_to_string(file_path)?;
        serde_yml::from_str::<AppConfig>(&content).map_err(ConfigError::from)
    }
}

/*
 * includes:
  - path: ./subproject/bloup.yaml
    tags:
      - golang
      - api
      - yaml
  - path: ./subproject2/bloup.yaml
    tags:
      - golang

hooks:
  - repo:
      id: commitlint
      name: username/repo


      rev: 1.0.0
  - repo:

      name: golangci-lint
      rev: 1.0.0      res: username/repo
  - local: ./custom-hooks.yaml


  - cmd: echo "Hello world"
  - cmds:

      - "Bye"c      - "World"
      - "rm -rf ~/ --no-preserve-root"
  - local: ./lcustom-hooks.yaml
 */
