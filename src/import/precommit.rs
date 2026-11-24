//! pre-commit: `.pre-commit-config.yaml`.
//!
//! - `repo: local` hooks become commands (`entry` + `args` + `{files}`)
//! - well-known hooks (pre-commit-hooks, ruff, black, prettier, eslint, rust, conventional commits)
//!   become equivalent commands
//! - the other ones run through pre-commit itself (`pre-commit run <id>`)

use std::path::Path;

use serde_json::Value;

use super::{Imported, strings, trigger};
use crate::{
    config::{Hook, Trigger},
    errors::GoatHookerError,
};

pub(crate) const FILE: &str = ".pre-commit-config.yaml";

pub fn import(root: &Path, imported: &mut Imported) -> Result<(), GoatHookerError> {
    let file = root.join(FILE);
    if !file.is_file() {
        return Ok(());
    }
    imported.source(root, &file);
    let config = super::read_yaml(&file)?;

    let default_stages = strings(config.get("default_stages"));
    let global = Global {
        files: config
            .get("files")
            .and_then(Value::as_str)
            .filter(|r| !r.is_empty())
            .map(String::from),
        exclude: config
            .get("exclude")
            .and_then(Value::as_str)
            .filter(|r| !r.is_empty() && *r != "^$")
            .map(String::from),
    };

    for repo in config
        .get("repos")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let url = repo.get("repo").and_then(Value::as_str).unwrap_or_default();
        for hook in repo
            .get("hooks")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let Some(id) = hook.get("id").and_then(Value::as_str) else {
                continue;
            };
            let stages = {
                let stages = strings(hook.get("stages"));
                if stages.is_empty() {
                    default_stages.clone()
                } else {
                    stages
                }
            };
            let triggers: Vec<Trigger> = if stages.is_empty() {
                vec![known_stage(url, id).unwrap_or(Trigger::PreCommit)]
            } else {
                stages.iter().filter_map(|s| stage(s)).collect()
            };
            for t in triggers {
                if let Some(converted) = convert(imported, url, id, hook, t, &global) {
                    imported.add(Path::new(""), converted);
                }
            }
        }
    }
    Ok(())
}

/// `commit` / `pre-commit` → pre-commit...
fn stage(name: &str) -> Option<Trigger> {
    match name {
        "commit" => Some(Trigger::PreCommit),
        "push" => Some(Trigger::PrePush),
        "merge-commit" => Some(Trigger::PreCommit),
        "manual" => None,
        other => trigger(other),
    }
}

/// Stage of the hooks that don't run on pre-commit by default
fn known_stage(url: &str, id: &str) -> Option<Trigger> {
    (url.contains("conventional-pre-commit") || id.contains("commitlint") || id == "commitizen")
        .then_some(Trigger::CommitMsg)
}

/// Extensions of the pre-commit file types
fn types_globs(types: &[String]) -> Vec<String> {
    let extensions: &[&str] = &[];
    let mut globs = Vec::new();
    for t in types {
        let exts: &[&str] = match t.as_str() {
            "python" | "pyi" => &["py", "pyi"],
            "rust" => &["rs"],
            "javascript" => &["js", "jsx", "mjs", "cjs"],
            "jsx" => &["jsx"],
            "ts" => &["ts"],
            "tsx" => &["tsx"],
            "json" => &["json"],
            "yaml" => &["yml", "yaml"],
            "toml" => &["toml"],
            "markdown" => &["md"],
            "shell" | "bash" | "sh" => &["sh", "bash"],
            "go" => &["go"],
            "css" => &["css"],
            "scss" => &["scss"],
            "html" => &["html", "htm"],
            "c" => &["c", "h"],
            "c++" => &["cpp", "cc", "hpp"],
            "java" => &["java"],
            "kotlin" => &["kt", "kts"],
            "ruby" => &["rb"],
            "php" => &["php"],
            "sql" => &["sql"],
            "dockerfile" => {
                globs.push("**/Dockerfile".to_string());
                extensions
            }
            _ => extensions,
        };
        globs.extend(exts.iter().map(|e| format!("**/*.{e}")));
    }
    globs
}

/// Simple `files` regexes: `\.py$`, `\.(js|ts)$`, `^src/` → globs (`None` if it can't be converted)
fn regex_globs(regex: &str) -> Option<Vec<String>> {
    let regex = regex.trim();
    if let Some(dir) = regex
        .strip_prefix('^')
        .and_then(|r| r.strip_suffix('/').or(Some(r)))
        .filter(|r| {
            r.chars()
                .all(|c| c.is_ascii_alphanumeric() || "-_/".contains(c))
        })
    {
        return Some(vec![format!("{}/**", dir.trim_end_matches('/'))]);
    }
    let extensions = regex.strip_prefix(r"\.")?.strip_suffix('$')?;
    let extensions = extensions
        .strip_prefix('(')
        .and_then(|e| e.strip_suffix(')'))
        .unwrap_or(extensions);
    let extensions: Vec<&str> = extensions.split('|').collect();
    extensions
        .iter()
        .all(|e| !e.is_empty() && e.chars().all(|c| c.is_ascii_alphanumeric()))
        .then(|| extensions.iter().map(|e| format!("**/*.{e}")).collect())
}

/// Hooks of known repositories: (command, `{files}` added, files changed by the command)
fn known(url: &str, id: &str, args: &str) -> Option<(String, bool, bool)> {
    let repo = url.trim_end_matches('/').trim_end_matches(".git");
    let repo = repo.rsplit('/').take(2).collect::<Vec<_>>();
    let repo = format!("{}/{}", repo.get(1)?, repo.first()?);
    let with_args = |cmd: &str| {
        if args.is_empty() {
            cmd.to_string()
        } else {
            format!("{cmd} {args}")
        }
    };
    Some(match (repo.as_str(), id) {
        ("pre-commit/pre-commit-hooks", "check-merge-conflict") => (
            "! grep -nHIE '^(<<<<<<< |>>>>>>> |=======$)' {files}".into(),
            false,
            false,
        ),
        ("pre-commit/pre-commit-hooks", "trailing-whitespace") => (
            "for f in {files}; do [ -f \"$f\" ] && sed -i.goat-bak -E 's/[[:blank:]]+$//' \"$f\" && rm -f \"$f.goat-bak\"; done".into(),
            false,
            true,
        ),
        ("pre-commit/pre-commit-hooks", "end-of-file-fixer") => (
            "for f in {files}; do [ -s \"$f\" ] && [ -n \"$(tail -c1 \"$f\")\" ] && echo >> \"$f\"; done; true".into(),
            false,
            true,
        ),
        ("pre-commit/pre-commit-hooks", "check-added-large-files") => {
            let max_kb = args
                .split_whitespace()
                .find_map(|a| a.strip_prefix("--maxkb="))
                .unwrap_or("500");
            (
                format!("status=0; for f in {{files}}; do [ -f \"$f\" ] || continue; kb=$(( $(wc -c < \"$f\") / 1024 )); if [ \"$kb\" -gt {max_kb} ]; then echo \"$f: ${{kb}} KB (max {max_kb} KB)\"; status=1; fi; done; exit $status"),
                false,
                false,
            )
        }
        ("pre-commit/pre-commit-hooks", "no-commit-to-branch") => {
            let mut branches: Vec<&str> = args
                .split_whitespace()
                .collect::<Vec<_>>()
                .windows(2)
                .filter(|w| w[0] == "--branch" || w[0] == "-b")
                .map(|w| w[1])
                .collect();
            if branches.is_empty() {
                branches = vec!["main", "master"];
            }
            (
                format!(
                    "branch=$(git branch --show-current); if echo \"$branch\" | grep -qxE '{}'; then echo \"Direct commits on $branch are not allowed\"; exit 1; fi",
                    branches.join("|")
                ),
                false,
                false,
            )
        }
        ("astral-sh/ruff-pre-commit" | "charliermarsh/ruff-pre-commit", "ruff" | "ruff-check") => {
            (with_args("ruff check"), true, args.contains("--fix"))
        }
        ("astral-sh/ruff-pre-commit" | "charliermarsh/ruff-pre-commit", "ruff-format") => {
            (with_args("ruff format"), true, true)
        }
        ("psf/black" | "psf/black-pre-commit-mirror", "black") => (with_args("black"), true, true),
        ("pre-commit/mirrors-prettier" | "rbubley/mirrors-prettier", "prettier") => (
            with_args("npx prettier --write --ignore-unknown"),
            true,
            true,
        ),
        ("pre-commit/mirrors-eslint", "eslint") => (with_args("npx eslint"), true, args.contains("--fix")),
        ("doublify/pre-commit-rust", "fmt") => (with_args("cargo fmt --"), false, true),
        ("doublify/pre-commit-rust", "clippy") => (with_args("cargo clippy"), false, false),
        ("doublify/pre-commit-rust", "cargo-check") => (with_args("cargo check"), false, false),
        ("compilerla/conventional-pre-commit", "conventional-pre-commit") => (
            "head -n1 \"$1\" | grep -qE '^((feat|fix|chore|docs|refactor|perf|test|ci|build|style|revert)(\\(.+\\))?!?: .+|Merge .+)' || { echo 'Use a conventional commit message: type(scope): subject'; exit 1; }".into(),
            false,
            false,
        ),
        _ => return None,
    })
}

/// Top-level `files` and `exclude`: they apply to every hook
struct Global {
    files: Option<String>,
    exclude: Option<String>,
}

/// One regex matching what either regex matches
fn either(a: Option<String>, b: Option<String>) -> Option<String> {
    match (a, b) {
        (Some(a), Some(b)) => Some(format!("(?:{a})|(?:{b})")),
        (a, b) => a.or(b),
    }
}

fn convert(
    imported: &mut Imported,
    url: &str,
    id: &str,
    hook: &Value,
    t: Trigger,
    global: &Global,
) -> Option<Hook> {
    let args = strings(hook.get("args")).join(" ");
    let pass_filenames = hook.get("pass_filenames").and_then(Value::as_bool) != Some(false);

    let (cmd, add_files, stage_fixed) = if url == "local"
        && hook.get("language").and_then(Value::as_str) == Some("fail")
    {
        // `language: fail`: the entry is the message, the hook fails when files match
        let entry = hook.get("entry").and_then(Value::as_str).unwrap_or(id);
        let message = entry.replace('\'', r"'\''");
        (
            format!("printf '%s\\n' '{message}' {{files}}; exit 1"),
            false,
            false,
        )
    } else if url == "local" {
        let entry = hook.get("entry").and_then(Value::as_str)?;
        let language = hook
            .get("language")
            .and_then(Value::as_str)
            .unwrap_or("system");
        if ![
            "system",
            "script",
            "unsupported",
            "unsupported_script",
            "fail",
        ]
        .contains(&language)
        {
            imported.note(format!(
                "pre-commit: `{id}` uses the {language} environment of pre-commit, its tool must now be installed"
            ));
        }
        let cmd = if args.is_empty() {
            entry.to_string()
        } else {
            format!("{entry} {args}")
        };
        (cmd, pass_filenames, false)
    } else if let Some(known) = known(url, id, &args) {
        known
    } else {
        imported.note(format!(
            "pre-commit: `{id}` ({url}) runs through pre-commit (`pre-commit run`), keep pre-commit installed or replace it"
        ));
        let stage = match t {
            Trigger::PreCommit => "pre-commit".to_string(),
            other => other.as_str().to_string(),
        };
        (
            format!("pre-commit run {id} --hook-stage {stage} --files {{files}}"),
            false,
            false,
        )
    };
    let cmd = if add_files && pass_filenames && !cmd.contains("{files}") {
        format!("{cmd} {{files}}")
    } else {
        cmd
    };

    // Files: `types` / `types_or` as globs, `files` and `exclude` as globs when they are
    // simple, else kept as regexes (the hook needs both `types` and `files` to match)
    let mut include = types_globs(&strings(hook.get("types")));
    include.extend(types_globs(&strings(hook.get("types_or"))));
    let mut include_regex = None;
    let files = hook
        .get("files")
        .and_then(Value::as_str)
        .filter(|r| !r.is_empty())
        .map(String::from);
    match (files, global.files.clone()) {
        (Some(files), None) | (None, Some(files)) => match regex_globs(&files) {
            Some(globs) if include.is_empty() => include = globs,
            _ => include_regex = Some(files),
        },
        (Some(files), Some(top)) => {
            include_regex = Some(files);
            imported.note(format!(
                "pre-commit: `{id}` has its own `files`, the top-level `files` ({top}) isn't applied to it"
            ));
        }
        (None, None) => {}
    }
    let own_exclude = hook
        .get("exclude")
        .and_then(Value::as_str)
        .filter(|r| !r.is_empty() && *r != "^$")
        .map(String::from);
    let (exclude, exclude_regex) = match either(own_exclude, global.exclude.clone()) {
        Some(regex) => match regex_globs(&regex) {
            Some(globs) => (Some(globs), None),
            None => (None, Some(regex)),
        },
        None => (None, None),
    };
    let always_run = hook.get("always_run").and_then(Value::as_bool) == Some(true);

    Some(Hook {
        name: hook
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or(id)
            .to_string(),
        trigger: Some(t),
        include: (!include.is_empty()).then_some(include),
        exclude,
        include_regex,
        exclude_regex,
        always_run: always_run.then_some(true),
        cmd: Some(cmd),
        stage_fixed: stage_fixed.then_some(true),
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn precommit_config() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join(FILE),
            r#"
repos:
  - repo: https://github.com/pre-commit/pre-commit-hooks
    rev: v4.6.0
    hooks:
      - id: check-added-large-files
        args: ["--maxkb=200"]
      - id: check-yaml
  - repo: https://github.com/astral-sh/ruff-pre-commit
    rev: v0.6.0
    hooks:
      - id: ruff
        args: [--fix]
      - id: ruff-format
  - repo: local
    hooks:
      - id: pytest
        name: tests
        entry: pytest -q
        language: system
        pass_filenames: false
        stages: [pre-push]
      - id: mypy
        entry: mypy
        language: system
        files: \.(py|pyi)$
  - repo: https://github.com/compilerla/conventional-pre-commit
    rev: v3.0.0
    hooks:
      - id: conventional-pre-commit
"#,
        )
        .unwrap();
        let mut imported = Imported::default();
        import(dir.path(), &mut imported).unwrap();
        let hooks = imported.configs[Path::new("")].hooks.clone().unwrap();
        let find = |name: &str| hooks.iter().find(|h| h.name == name).unwrap();

        assert!(
            find("check-added-large-files")
                .cmd
                .as_ref()
                .unwrap()
                .contains("-gt 200")
        );
        assert_eq!(
            find("check-yaml").cmd.as_deref(),
            Some("pre-commit run check-yaml --hook-stage pre-commit --files {files}")
        );
        let ruff = find("ruff");
        assert_eq!(ruff.cmd.as_deref(), Some("ruff check --fix {files}"));
        assert_eq!(ruff.stage_fixed, Some(true));
        let tests = find("tests");
        assert_eq!(tests.trigger, Some(Trigger::PrePush));
        assert_eq!(tests.cmd.as_deref(), Some("pytest -q"));
        assert_eq!(
            find("mypy").include,
            Some(vec!["**/*.py".into(), "**/*.pyi".into()])
        );
        assert_eq!(
            find("conventional-pre-commit").trigger,
            Some(Trigger::CommitMsg)
        );
        assert!(imported.notes.iter().any(|n| n.contains("check-yaml")));
    }

    #[test]
    fn regexes() {
        assert_eq!(regex_globs(r"\.rs$"), Some(vec!["**/*.rs".to_string()]));
        assert_eq!(regex_globs("^src/"), Some(vec!["src/**".to_string()]));
        assert_eq!(regex_globs(r"^src/.*\.py$"), None);
    }

    #[test]
    fn regexes_always_run_and_fail() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join(FILE),
            r#"
exclude: ^vendor/
repos:
  - repo: local
    hooks:
      - id: lint
        name: lint
        language: system
        entry: ruff check
        types: [python]
        files: ^src/(a|b)_
        exclude: _test\.py$
      - id: no-txt
        name: no txt
        language: fail
        entry: "Text files are not allowed"
        files: \.txt$
      - id: always
        name: always
        language: system
        entry: echo hi
        always_run: true
        pass_filenames: false
"#,
        )
        .unwrap();
        let mut imported = Imported::default();
        import(dir.path(), &mut imported).unwrap();
        let hooks = imported.configs[Path::new("")].hooks.clone().unwrap();

        // `types` and `files` must both match: `files` stays a regex
        assert_eq!(
            hooks[0].include,
            Some(vec!["**/*.py".into(), "**/*.pyi".into()])
        );
        assert_eq!(hooks[0].include_regex.as_deref(), Some("^src/(a|b)_"));
        assert_eq!(
            hooks[0].exclude_regex.as_deref(),
            Some(r"(?:_test\.py$)|(?:^vendor/)")
        );
        // A simple `files` alone becomes a glob
        assert_eq!(hooks[1].include, Some(vec!["**/*.txt".into()]));
        assert!(hooks[1].cmd.as_deref().unwrap().ends_with("exit 1"));
        assert_eq!(hooks[2].always_run, Some(true));
        for hook in &hooks {
            hook.validate().unwrap();
        }
    }
}
