//! `hooker doctor`: what could go wrong with the hooks of a repository, and how to fix it.
//! Used by the CLI and the Doctor page of the app.

use std::{
    collections::BTreeMap,
    env,
    path::{Path, PathBuf},
    time::Duration,
};

use crate::{
    config::{Config, Hook, LOCAL_CONFIG_FILENAMES, Trigger},
    git,
    graph::project_name,
    history::{self, Status},
    install::HOOK_MARKER,
    matcher::Matcher,
    trust, variables,
};

/// Average duration above which a pre-commit hook is worth a hint
const SLOW: Duration = Duration::from_secs(5);
/// Runs read from the history to give hints
const HISTORY: usize = 200;

/// Commands run by the shell itself: nothing to look for in `PATH`
const BUILTINS: &[&str] = &[
    ".", ":", "[", "[[", "alias", "break", "case", "cd", "command", "continue", "echo", "eval",
    "exec", "exit", "export", "false", "for", "if", "local", "printf", "pwd", "read", "return",
    "set", "shift", "source", "test", "trap", "true", "type", "ulimit", "umask", "unset", "until",
    "wait", "while", "{", "(", "!",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    /// Hooks won't run, or will fail
    Error,
    /// Something is probably wrong
    Warning,
    /// Could be better
    Hint,
}

#[derive(Debug, Clone)]
pub struct Finding {
    pub level: Level,
    pub title: String,
    pub detail: Option<String>,
    /// Command fixing it, when there is one
    pub fix: Option<String>,
}

impl Finding {
    fn new(level: Level, title: impl Into<String>) -> Self {
        Self {
            level,
            title: title.into(),
            detail: None,
            fix: None,
        }
    }

    fn detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    fn fix(mut self, fix: impl Into<String>) -> Self {
        self.fix = Some(fix.into());
        self
    }
}

/// Every finding, the most serious first
pub fn diagnose(root: &Path, configs: &[Config]) -> Vec<Finding> {
    let mut findings = Vec::new();
    findings.extend(installation(root, configs));
    findings.extend(missing_tools(root, configs));
    findings.extend(unmatched_patterns(root, configs));
    findings.extend(unused_variables(root, configs));
    findings.extend(untrusted(root, configs));
    findings.extend(from_history(root, configs));
    findings.extend(committed_local(root));
    findings.extend(foreign(root, configs));
    findings.sort_by_key(|f| f.level);
    findings
}

fn hooks(configs: &[Config]) -> impl Iterator<Item = (&Config, &Hook)> {
    configs
        .iter()
        .flat_map(|c| c.hooks.iter().flatten().map(move |h| (c, h)))
}

fn label(root: &Path, config: &Config, hook: &Hook) -> String {
    let project = config
        .base_dir
        .as_deref()
        .map(|dir| project_name(root, dir))
        .unwrap_or_else(|| "root".into());
    format!("`{}` ({project})", hook.name)
}

/// The config of another tool, read as it is: what couldn't be read exactly
fn foreign(root: &Path, configs: &[Config]) -> Vec<Finding> {
    let Some(file) = configs
        .first()
        .filter(|c| c.is_foreign())
        .and_then(|c| c.file.as_ref())
    else {
        return vec![];
    };
    let shown = file
        .strip_prefix(root)
        .unwrap_or(file)
        .display()
        .to_string();
    let notes = crate::import::detect(root)
        .map(|i| i.notes)
        .unwrap_or_default();
    let mut findings = vec![
        Finding::new(Level::Hint, format!("the hooks come from {shown}, read as it is"))
            .detail("nothing to migrate; `hooker import` writes a hooker.toml to use Goat Hooker's own features")
            .fix("hooker import"),
    ];
    findings.extend(notes.into_iter().map(|note| {
        Finding::new(Level::Warning, note).detail(format!("{shown} is read as it is"))
    }));
    findings
}

/// Git hooks missing, overwritten, or bypassed by `core.hooksPath`
fn installation(root: &Path, configs: &[Config]) -> Vec<Finding> {
    let mut findings = Vec::new();
    if let Some(path) = git::hooks_path_setting(root) {
        findings.push(
            Finding::new(Level::Warning, format!("core.hooksPath is set to `{path}`"))
                .detail("Git runs the hooks of that folder: another hooks manager (husky...) may still be in charge")
                .fix("git config --unset core.hooksPath && hooker install"),
        );
    }
    let Ok(dir) = git::hooks_dir(root) else {
        return findings;
    };
    let mut triggers: Vec<Trigger> = hooks(configs).filter_map(|(_, h)| h.trigger).collect();
    triggers.sort_by_key(|t| t.as_str());
    triggers.dedup();
    for trigger in triggers {
        match std::fs::read_to_string(dir.join(trigger.as_str())) {
            Ok(content) if content.contains(HOOK_MARKER) => {}
            Ok(_) => findings.push(
                Finding::new(
                    Level::Error,
                    format!(
                        "the {} Git hook isn't the one of goat-hooker",
                        trigger.as_str()
                    ),
                )
                .detail("a hand-written hook is there: the hooks of the config don't run")
                .fix("hooker install --force"),
            ),
            Err(_) => findings.push(
                Finding::new(
                    Level::Error,
                    format!("the {} Git hook isn't installed", trigger.as_str()),
                )
                .detail("the hooks of this trigger never run")
                .fix("hooker install"),
            ),
        }
    }
    findings
}

/// First word of a command that is looked for in `PATH`
fn program(command: &str) -> Option<&str> {
    command
        .split_whitespace()
        // `FOO=bar cmd`: the variables come first
        .find(|word| !word.contains('=') || word.starts_with('='))
        .filter(|word| {
            !BUILTINS.contains(word)
                && !word.starts_with(['$', '{', '"', '\'', '`', '-', '('])
                && !word.contains('/')
                && !word.contains('\\')
        })
}

fn in_path(program: &str) -> bool {
    let Some(path) = env::var_os("PATH") else {
        return false;
    };
    let extensions: &[&str] = if cfg!(windows) {
        &["", ".exe", ".cmd", ".bat"]
    } else {
        &[""]
    };
    env::split_paths(&path).any(|dir| {
        extensions
            .iter()
            .any(|ext| dir.join(format!("{program}{ext}")).is_file())
    })
}

/// Programs the commands start with, but that aren't installed
fn missing_tools(root: &Path, configs: &[Config]) -> Vec<Finding> {
    let mut missing: BTreeMap<&str, Vec<String>> = BTreeMap::new();
    for (config, hook) in hooks(configs) {
        for command in hook.commands() {
            if let Some(program) = program(command)
                && !in_path(program)
            {
                let hooks = missing.entry(program).or_default();
                let label = label(root, config, hook);
                if !hooks.contains(&label) {
                    hooks.push(label);
                }
            }
        }
    }
    missing
        .into_iter()
        .map(|(program, hooks)| {
            Finding::new(Level::Error, format!("`{program}` isn't installed"))
                .detail(format!("used by {}", hooks.join(", ")))
        })
        .collect()
}

/// Tracked files of the repository, as absolute paths
fn tracked(root: &Path) -> Vec<PathBuf> {
    std::process::Command::new("git")
        .args(["ls-files", "-z"])
        .current_dir(root)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .split('\0')
                .filter(|f| !f.is_empty())
                .map(|f| root.join(f))
                .collect()
        })
        .unwrap_or_default()
}

/// Hooks whose `include` matches no tracked file: probably a typo in a pattern
fn unmatched_patterns(root: &Path, configs: &[Config]) -> Vec<Finding> {
    let Ok(matcher) = Matcher::new(configs) else {
        return vec![];
    };
    let files = tracked(root);
    if files.is_empty() {
        return vec![];
    }
    let mut findings = Vec::new();
    for trigger in [Trigger::PreCommit, Trigger::PrePush] {
        let matched = matcher.hooks_to_run(trigger, Some(&files));
        for (config, hook) in hooks(configs) {
            if hook.trigger != Some(trigger) || hook.include.is_none() {
                continue;
            }
            if !matched.iter().any(|m| std::ptr::eq(m.hook, hook)) {
                findings.push(
                    Finding::new(
                        Level::Warning,
                        format!("{} matches no file", label(root, config, hook)),
                    )
                    .detail(format!(
                        "include: {} — it never runs",
                        hook.include
                            .iter()
                            .flatten()
                            .cloned()
                            .collect::<Vec<_>>()
                            .join(", ")
                    )),
                );
            }
        }
    }
    findings
}

/// Variables nobody reads or writes, and denied accesses
fn unused_variables(root: &Path, configs: &[Config]) -> Vec<Finding> {
    let Ok(usages) = variables::usage(root, configs) else {
        return vec![];
    };
    let mut findings = Vec::new();
    for usage in usages {
        let name = &usage.declared.name;
        if usage.readers.is_empty() && usage.writers.is_empty() {
            findings.push(
                Finding::new(Level::Hint, format!("variable `{name}` is never used"))
                    .detail("no hook reads or writes it: it can be removed"),
            );
        } else if usage.readers.is_empty() {
            findings.push(
                Finding::new(Level::Hint, format!("variable `{name}` is never read"))
                    .detail(format!("written by {}", usage.writers.join(", "))),
            );
        } else if usage.writers.is_empty() && usage.value.is_none() {
            findings.push(
                Finding::new(
                    Level::Warning,
                    format!("variable `{name}` is never written"),
                )
                .detail(format!(
                    "{} read it, but it has no value and no default",
                    usage.readers.join(", ")
                )),
            );
        }
        for violation in usage.violations.iter().chain(&usage.denied) {
            findings.push(
                Finding::new(Level::Warning, format!("variable `{name}`: access denied"))
                    .detail(violation.clone()),
            );
        }
    }
    findings
}

/// Hooks of other repositories that won't run until they are trusted
fn untrusted(root: &Path, configs: &[Config]) -> Vec<Finding> {
    trust::untrusted(root, hooks(configs).map(|(_, h)| h))
        .into_iter()
        .map(|origin| {
            Finding::new(
                Level::Error,
                format!("hooks of {} aren't trusted", origin.url),
            )
            .detail(format!(
                "{} — new or changed since they were trusted, they don't run",
                origin.path
            ))
            .fix("hooker trust")
        })
        .collect()
}

/// `hooker.local.toml` (or `.json`) holds the changes of one computer: it must not be committed
fn committed_local(root: &Path) -> Vec<Finding> {
    tracked(root)
        .into_iter()
        .filter(|f| {
            f.file_name()
                .is_some_and(|n| LOCAL_CONFIG_FILENAMES.iter().any(|l| n == *l))
        })
        .map(|file| {
            let file = file
                .strip_prefix(root)
                .unwrap_or(&file)
                .display()
                .to_string();
            Finding::new(Level::Warning, format!("{file} is committed"))
                .detail("it holds the changes of one computer: everyone gets them")
                .fix(format!(
                    "git rm --cached {file} && echo 'hooker.local.*' >> .gitignore"
                ))
        })
        .collect()
}

/// Slow or often failing hooks, from the last runs
fn from_history(root: &Path, configs: &[Config]) -> Vec<Finding> {
    let runs = history::load(root, HISTORY);
    let mut stats: BTreeMap<(String, String), (u64, usize, usize)> = BTreeMap::new();
    for run in runs
        .iter()
        .filter(|r| r.trigger == Trigger::PreCommit.as_str())
    {
        for hook in &run.hooks {
            if matches!(hook.status, Status::Passed | Status::Failed) {
                let entry = stats
                    .entry((hook.project.clone(), hook.name.clone()))
                    .or_default();
                entry.0 += hook.duration_ms;
                entry.1 += 1;
                if hook.status == Status::Failed {
                    entry.2 += 1;
                }
            }
        }
    }

    let cached = |project: &str, name: &str| {
        hooks(configs).any(|(config, hook)| {
            hook.name == name
                && hook.cache == Some(true)
                && config
                    .base_dir
                    .as_deref()
                    .map(|dir| project_name(root, dir))
                    .unwrap_or_else(|| "root".into())
                    == project
        })
    };

    let mut findings = Vec::new();
    for ((project, name), (total, count, failed)) in stats {
        let average = Duration::from_millis(total / count as u64);
        if count >= 3 && average >= SLOW {
            let finding = Finding::new(
                Level::Hint,
                format!(
                    "`{name}` ({project}) takes {:.1}s on each commit",
                    average.as_secs_f64()
                ),
            );
            findings.push(if cached(&project, &name) {
                finding.detail("`hooker watch` runs it as soon as files are staged, so the commit takes it from the cache")
            } else {
                finding.detail(
                    "add `\"cache\": true` to skip it when its files didn't change, or move it to pre-push",
                )
            });
        }
        if count >= 4 && failed * 2 > count {
            findings.push(
                Finding::new(
                    Level::Hint,
                    format!("`{name}` ({project}) failed {failed} of its last {count} runs"),
                )
                .detail("if it fixes the files itself, `\"stage_fixed\": true` stages the fixes"),
            );
        }
    }
    findings
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_program_of_a_command() {
        assert_eq!(program("cargo fmt --check"), Some("cargo"));
        assert_eq!(program("RUST_LOG=1 cargo test"), Some("cargo"));
        assert_eq!(program("./scripts/lint.sh"), None);
        assert_eq!(program("echo done"), None);
        assert_eq!(program("$HOME/bin/tool"), None);
        assert_eq!(program("test -f x && cargo test"), None);
    }

    #[test]
    fn tools_not_in_path_are_reported() {
        let config: Config = serde_json::from_str(
            r#"{ "hooks": [
                { "name": "ok", "trigger": "pre-commit", "cmd": "sh -c true" },
                { "name": "ko", "trigger": "pre-commit", "cmd": "no-such-tool-xyz --check" }
            ] }"#,
        )
        .unwrap();
        let findings = missing_tools(Path::new("/repo"), &[config]);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].title, "`no-such-tool-xyz` isn't installed");
        assert_eq!(findings[0].detail.as_deref(), Some("used by `ko` (root)"));
    }
}
