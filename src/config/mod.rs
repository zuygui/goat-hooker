use std::{fs, io, path::Path};

use rules::HookRule;
use serde::{Deserialize, Serialize};

mod rules;

pub const HOOK_CONFIG_FILENAME: &str = "hooks.config.yaml";

#[derive(Serialize, Deserialize, Debug)]
pub struct AppConfig {
    hooks: Vec<HookRule>,
}

impl Default for AppConfig {
    fn default() -> Self {
        AppConfig { hooks: Vec::new() }
    }
}

impl AppConfig {
    pub fn write_config(self) -> io::Result<()> {
        // Save the configuration
        let str_config = serde_yml::to_string(&self).unwrap();
        fs::write(HOOK_CONFIG_FILENAME, str_config)
    }

    pub fn exists() -> io::Result<bool> {
        fs::exists(HOOK_CONFIG_FILENAME)
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
