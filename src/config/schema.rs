use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Config {
    pub hooks: Vec<Hook>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Hook {
    Command {
        name: String,
        trigger: Trigger,

        #[serde(default)]
        include: Vec<String>,

        #[serde(default)]
        cmd: Option<String>,

        #[serde(default)]
        cmds: Vec<String>,
    },

    Repo {
        name: String,
        from: String,
        path: String,
    },
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]

pub enum Trigger {
    PreCommit,
    PrepareCommitMsg,
    CommitMsg,
    PostCommit,
    ApplyPatchMsg,
    PreApplyPatch,
    PostApplyPatch,
    PreRebase,
    PostRewrite,
    PostCheckout,
    PostMerge,
    PrePush,
    PreAutoGc,
    PreReceive,
    Update,
    PostReceive,
}
