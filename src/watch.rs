//! `hooker watch`: runs the pre-commit hooks in the background as soon as files are staged,
//! so that `git commit` finds their results in the cache and finishes at once.
//!
//! Only the hooks that the commit would take from the cache are run: cached, targeting files,
//! not modifying them (`stage_fixed`), without links (`needs`, `inputs`, variables). They run
//! in place, so only when the staged files have no unstaged changes: the hooks then see
//! exactly what will be committed, and the cache keys are the ones of the commit.
//! Nothing is put aside or staged.

use std::{
    path::{Path, PathBuf},
    time::SystemTime,
};

use crate::{
    config::{Config, Hook, Trigger},
    errors::GoatHookerError,
    git,
    runner::{self, HookInput, HookResult, Output},
};

/// Can this hook run ahead of the commit?
pub fn ahead(hook: &Hook) -> bool {
    hook.trigger == Some(Trigger::PreCommit)
        && hook.include.is_some()
        && hook.cache != Some(false)
        && hook.stage_fixed != Some(true)
        && hook.path.is_none()
        && hook.steps.is_none()
        && hook.needs.as_ref().is_none_or(|n| n.is_empty())
        && hook.inputs.as_ref().is_none_or(|i| i.is_empty())
        && hook.reads.as_ref().is_none_or(|r| r.is_empty())
        && hook.writes.as_ref().is_none_or(|w| w.is_empty())
}

/// The configs with only the hooks that can run ahead
pub fn prepare(configs: &[Config]) -> Vec<Config> {
    configs
        .iter()
        .cloned()
        .map(|mut config| {
            if let Some(hooks) = config.hooks.as_mut() {
                hooks.retain(ahead);
            }
            config
        })
        .collect()
}

/// What changes when files are staged or a commit is made
pub fn state(root: &Path) -> Option<(SystemTime, u64)> {
    let index = git::git_file(root, "index").ok()?;
    let metadata = std::fs::metadata(index).ok()?;
    Some((metadata.modified().ok()?, metadata.len()))
}

pub enum Round {
    /// Nothing staged
    Nothing,
    /// A commit, a rebase... is in progress
    Busy,
    /// These staged files also have unstaged changes: the hooks would not see what will be committed
    Unstaged(Vec<PathBuf>),
    Ran(Vec<HookResult>),
}

/// Runs the hooks of `prepared` (see `prepare`) on the staged files
pub fn run(root: &Path, prepared: &[Config]) -> Result<Round, GoatHookerError> {
    if git::git_file(root, "index.lock").is_ok_and(|lock| lock.exists()) {
        return Ok(Round::Busy);
    }
    let staged = git::staged_files(root)?;
    if staged.is_empty() {
        return Ok(Round::Nothing);
    }
    let unstaged = git::unstaged_files(root)?;
    let mixed: Vec<PathBuf> = staged
        .iter()
        .filter(|f| unstaged.contains(f))
        .cloned()
        .collect();
    if !mixed.is_empty() {
        return Ok(Round::Unstaged(mixed));
    }
    let input = HookInput {
        ci_files: Some(staged),
        ..Default::default()
    };
    let results = runner::run_trigger(
        root,
        prepared,
        Trigger::PreCommit,
        &input,
        Output::Capture,
        None,
    )?;
    Ok(Round::Ran(results))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hook(json: &str) -> Hook {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn only_read_only_cached_hooks_run_ahead() {
        let lint =
            r#"{ "name": "lint", "trigger": "pre-commit", "cmd": "lint", "include": ["*.rs"] "#;
        assert!(ahead(&hook(&format!("{lint} }}"))));
        assert!(!ahead(&hook(&format!(r#"{lint}, "stage_fixed": true }}"#))));
        assert!(!ahead(&hook(&format!(r#"{lint}, "cache": false }}"#))));
        assert!(!ahead(&hook(&format!(r#"{lint}, "needs": ["fmt"] }}"#))));
        assert!(!ahead(&hook(&format!(r#"{lint}, "writes": ["v"] }}"#))));
        assert!(!ahead(&hook(
            r#"{ "name": "test", "trigger": "pre-push", "cmd": "t", "include": ["*.rs"] }"#
        )));
        assert!(!ahead(&hook(
            r#"{ "name": "all", "trigger": "pre-commit", "cmd": "t" }"#
        )));
    }
}
