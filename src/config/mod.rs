pub mod loader;

use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Config {
    pub includes: Option<Vec<String>>,
    pub hooks: Option<Vec<Hook>>,
}

#[derive(Debug, Deserialize)]
pub struct Hook {
    pub name: String,
    pub trigger: Trigger,
    pub include: Option<Vec<String>>,
    pub cmd: Option<String>,
    pub cmds: Option<Vec<String>>,
    pub from: Option<String>,
    pub path: Option<String>,
}

#[derive(Debug, Deserialize)]
pub enum Trigger {
    #[serde(rename = "pre-commit")]
    PreCommit,
}
