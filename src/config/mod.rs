use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub struct AppConfig {}

impl Default for AppConfig {
    fn default() -> Self {
        AppConfig {}
    }
}

impl AppConfig {
    pub fn write_config(self) {
        // Save the configuration
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