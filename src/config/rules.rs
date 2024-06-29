use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub(crate) enum HookRule {
    Command(CommandHookRule),
}

#[derive(Serialize, Deserialize, Debug)]
pub enum CommandHookRule {
    Single(String),
    Multiple(Vec<String>),
}
