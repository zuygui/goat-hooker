//! `hooker fix`: runs the hooks that fix files (`stage_fixed`: formatters, `eslint --fix`...)
//! on the files you changed, without committing or staging anything.

use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

use crate::{
    config::{Config, Trigger},
    errors::GoatHookerError,
    git, graph,
    matcher::Matcher,
    runner::{self, HookInput, HookResult, OnProgress, Output, Shared},
};

/// The configs with only the pre-commit hooks fixing files. They keep the order between
/// them (`needs` on another fixer); the other links are dropped.
pub fn fixers(configs: &[Config]) -> Vec<Config> {
    let names: BTreeSet<String> = configs
        .iter()
        .flat_map(|c| c.hooks.iter().flatten())
        .filter(|h| is_fixer(h))
        .map(|h| h.name.clone())
        .collect();
    configs
        .iter()
        .cloned()
        .map(|mut config| {
            if let Some(hooks) = config.hooks.as_mut() {
                hooks.retain(is_fixer);
                for hook in hooks {
                    if let Some(needs) = hook.needs.as_mut() {
                        needs.retain(|n| names.contains(n.rsplit(':').next().unwrap_or(n)));
                    }
                    hook.inputs = None;
                    hook.reads = None;
                    hook.writes = None;
                }
            }
            config
        })
        .collect()
}

fn is_fixer(hook: &crate::config::Hook) -> bool {
    hook.trigger == Some(Trigger::PreCommit)
        && hook.stage_fixed == Some(true)
        && hook.steps.is_none()
}

/// Files changed since the last commit: staged, not staged and new (not ignored)
pub fn changed_files(root: &Path) -> Result<Vec<PathBuf>, GoatHookerError> {
    let mut files: BTreeSet<PathBuf> = git::staged_files(root)?.into_iter().collect();
    files.extend(git::unstaged_files(root)?);
    files.extend(git::untracked_files(root)?);
    Ok(files.into_iter().filter(|f| f.is_file()).collect())
}

pub struct Fixed {
    pub results: Vec<HookResult>,
    /// Files whose content changed, as absolute paths
    pub changed: Vec<PathBuf>,
}

/// Runs the fixers on `files` (the changed files when `None`)
pub fn run(
    root: &Path,
    configs: &[Config],
    files: Option<Vec<PathBuf>>,
    on_progress: Option<OnProgress>,
) -> Result<Fixed, GoatHookerError> {
    let files = match files {
        Some(files) => files,
        None => changed_files(root)?,
    };
    let prepared = fixers(configs);
    let matcher = Matcher::new(&prepared)?;
    let hooks = matcher.hooks_to_run(Trigger::PreCommit, Some(&files));
    if hooks.is_empty() {
        return Ok(Fixed {
            results: vec![],
            changed: vec![],
        });
    }

    let before: BTreeMap<&PathBuf, Option<Vec<u8>>> =
        files.iter().map(|f| (f, std::fs::read(f).ok())).collect();
    let graph = graph::build(&prepared, Trigger::PreCommit, &hooks)?;
    let shared = Shared::load(root, &prepared)?;
    let parallel = configs.first().and_then(|c| c.parallel).unwrap_or(false);
    let results = runner::run_graph(
        &graph,
        &HookInput::default(),
        parallel,
        Output::Capture,
        on_progress,
        &shared,
    )?;
    let changed = before
        .into_iter()
        .filter(|(file, content)| std::fs::read(file).ok() != *content)
        .map(|(file, _)| file.clone())
        .collect();
    Ok(Fixed { results, changed })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixers_run_on_changed_files_without_staging() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        let git = |args: &[&str]| {
            std::process::Command::new("git")
                .args(args)
                .current_dir(&root)
                .output()
                .unwrap()
        };
        git(&["init", "-q"]);
        git(&["config", "core.autocrlf", "false"]);
        std::fs::write(
            root.join("hooker.config.json"),
            r#"{ "hooks": [
                { "name": "upper", "trigger": "pre-commit", "include": ["*.txt"], "stage_fixed": true,
                  "cmd": "for f in {files}; do tr a-z A-Z < $f > $f.tmp && mv $f.tmp $f; done" },
                { "name": "lint", "trigger": "pre-commit", "include": ["*.txt"], "cmd": "false" }
            ] }"#,
        )
        .unwrap();
        std::fs::write(root.join("new.txt"), "hello\n").unwrap();
        std::fs::write(root.join("done.txt"), "DONE\n").unwrap();
        let configs = crate::config::ConfigLoader::new()
            .load_recursive(&root.join("hooker.config.json"))
            .unwrap();

        let fixed = run(&root, &configs, None, None).unwrap();
        // only the fixer ran
        assert_eq!(fixed.results.len(), 1);
        assert!(fixed.results[0].success);
        assert_eq!(fixed.changed, [root.join("new.txt")]);
        assert_eq!(
            std::fs::read_to_string(root.join("new.txt")).unwrap(),
            "HELLO\n"
        );
        // nothing staged
        assert!(git::staged_files(&root).unwrap().is_empty());
    }
}
