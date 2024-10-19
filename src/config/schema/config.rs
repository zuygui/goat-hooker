use std::collections::HashMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::rules::HookRule;

#[derive(Serialize, Deserialize, Hash, PartialEq, Eq, Debug, JsonSchema)]
pub enum HookType {
    #[serde(rename = "pre-commit")]
    PreCommit,
}

#[derive(Serialize, Deserialize, Debug, JsonSchema)]
#[serde(rename = "Goat Hooker config")]
pub struct AppConfig {
    pub hooks: HashMap<HookType, Vec<HookRule>>,
}
