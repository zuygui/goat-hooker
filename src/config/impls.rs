use serde_yml;
use std::{
    fs, io,
    path::{Path, PathBuf},
};

use super::{AppConfig, HOOK_CONFIG_FILENAME};

impl Default for AppConfig {
    fn default() -> Self {
        AppConfig { hooks: Vec::new() }
    }
}

impl AppConfig {
    pub fn write_config(self, path: PathBuf) -> io::Result<()> {
        // Save the configuration
        let str_config = serde_yml::to_string(&self).unwrap();
        fs::write(path.join(HOOK_CONFIG_FILENAME), str_config)
    }

    pub fn exists() -> bool {
        Path::new(HOOK_CONFIG_FILENAME).exists()
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
