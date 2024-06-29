use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::rules::HookRule;

#[derive(Serialize, Deserialize, Debug, JsonSchema)]
#[serde(rename = "Goat Hooker config")]
pub struct AppConfig {
    pub hooks: Vec<HookRule>,
}
