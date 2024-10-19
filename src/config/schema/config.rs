use std::collections::HashMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::rules::HookRule;

/// Here's the table  listing the Git hooks (client-side only), the corresponding file name, and a description of each:
///
/// | **Hook Name**          | **File Name**        | **Description**                                                                                  |
/// |------------------------|----------------------|--------------------------------------------------------------------------------------------------|
/// | Pre-commit             | `pre-commit`         | Executed before a commit is recorded. Used to check or modify files before the commit (e.g., linting or formatting). |
/// | Prepare-commit-msg     | `prepare-commit-msg`  | Executed before the commit message editor is opened. Used to modify or pre-fill the commit message. |
/// | Commit-msg             | `commit-msg`         | Executed after the commit message has been written, but before the commit is finalized. Used to validate or modify the message. |
/// | Post-commit            | `post-commit`        | Executed after a commit has been created. Often used for notifications or post-commit actions. |
/// | Pre-rebase             | `pre-rebase`         | Executed before a rebase begins. Can prevent or modify the rebase before execution. |
/// | Post-rewrite           | `post-rewrite`       | Executed after commands like `git commit --amend` or `git rebase`. Used for actions post-history rewrite. |
/// | Pre-push               | `pre-push`           | Executed before a `git push` is sent to the remote repository. Often used to run tests or checks before pushing. |
/// | Pre-applypatch         | `pre-applypatch`     | Executed before a patch is applied using `git am`. Used to perform checks before patch application. |
/// | Applypatch-msg         | `applypatch-msg`     | Executed after a patch has been applied but before the commit message is finalized. Used to validate or modify the patch message. |
/// | Post-applypatch        | `post-applypatch`    | Executed after a patch has been successfully applied. Used for actions after applying the patch. |
/// | Pre-auto-gc            | `pre-auto-gc`        | Executed before Git runs automatic garbage collection. Can be used to disable or delay the GC process. |
/// | Post-checkout          | `post-checkout`      | Executed after a branch or commit has been successfully checked out. Used for actions like updating environments or dependencies. |
/// | Post-merge             | `post-merge`         | Executed after a `git merge` is successfully completed. Often used for cleanup actions after merging. |
/// 
/// ### Notes:
/// - **File Name**: The file to create in the `.git/hooks/` directory to activate the hook.
/// - **Description**: A brief explanation of the hook's purpose and typical use cases.
#[derive(Serialize, Deserialize, Hash, PartialEq, Eq, Debug, JsonSchema)]
pub enum HookType {
    #[serde(rename = "pre-commit")]
    PreCommit,
    #[serde(rename = "prepare-commit-msg")]
    PrepareCommitMsg,
    #[serde(rename = "commit-msg")]
    CommitMsg,
    #[serde(rename = "post-commit")]
    PostCommit,
    #[serde(rename = "pre-rebase")]
    PreRebase,
    #[serde(rename = "post-rebase")]
    PostRewrite,
    #[serde(rename = "pre-push")]
    PrePush,
    #[serde(rename = "pre-applypatch")]
    PreApplyPatch,
    #[serde(rename = "post-applypatch")]
    PostApplyPatch,
    #[serde(rename = "pre-auto-gc")]
    PreAutoGc,
    #[serde(rename = "post-checkout")]
    PostCheckout,
    #[serde(rename = "post-merge")]
    PostMerge,
}

#[derive(Serialize, Deserialize, Debug, JsonSchema)]
#[serde(rename = "Goat Hooker config")]
pub struct AppConfig {
    pub hooks: HashMap<HookType, Vec<HookRule>>,
}
