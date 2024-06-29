use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, JsonSchema)]
#[serde(untagged)]
pub(crate) enum HookRule {
    Command(CommandHookRule),
}

#[derive(Serialize, Deserialize, Debug, JsonSchema)]
pub enum CommandHookRule {
    #[serde(rename = "cmd")]
    Single(String),
    #[serde(rename = "cmds")]
    Multiple(Vec<String>),
}
