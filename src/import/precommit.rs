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
        let rev = repo.get("rev").and_then(Value::as_str);
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
                if let Some(mut converted) = convert(imported, url, rev, id, hook, t, &global) {
                    // Without stages, a hook of a repository runs on the ones of its definition
                    if stages.is_empty() && converted.id.is_some() {
                        converted.trigger = None;
                    }
                    imported.add(Path::new(""), converted);
                }
            }
        }
    }
    Ok(())
}

/// `commit` / `pre-commit` → pre-commit...
pub(crate) fn stage(name: &str) -> Option<Trigger> {
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
/// File kinds of pre-commit, checked on the files themselves (`types` of a hook)
pub(crate) const KINDS: [&str; 7] = [
    "text",
    "binary",
    "executable",
    "non-executable",
    "symlink",
    "file",
    "directory",
];

/// The `types` that are kinds of files, not languages
pub(crate) fn kinds(types: &[String]) -> Vec<String> {
    types
        .iter()
        .filter(|t| KINDS.contains(&t.as_str()))
        .cloned()
        .collect()
}

pub(crate) fn types_globs(types: &[String]) -> Vec<String> {
    let extensions: &[&str] = &[];
    let mut globs = Vec::new();
    for t in types {
        if KINDS.contains(&t.as_str()) {
            continue;
        }
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
            "proto" => &["proto"],
            "terraform" => &["tf", "tfvars"],
            "xml" => &["xml"],
            "svg" => &["svg"],
            "csv" => &["csv"],
            "perl" => &["pl", "pm"],
            "swift" => &["swift"],
            "dart" => &["dart"],
            "scala" => &["scala", "sc"],
            "elixir" => &["ex", "exs"],
            "erlang" => &["erl", "hrl"],
            "haskell" => &["hs"],
            "ocaml" => &["ml", "mli"],
            "r" => &["r", "R"],
            "julia" => &["jl"],
            "clojure" => &["clj", "cljs", "cljc", "edn"],
            "less" => &["less"],
            "sass" => &["sass"],
            "graphql" => &["graphql", "gql"],
            "rst" => &["rst"],
            "asciidoc" => &["adoc", "asciidoc"],
            "ini" => &["ini", "cfg"],
            "cmake" => &["cmake"],
            "groovy" => &["groovy", "gradle"],
            "objective-c" => &["m", "mm"],
            "c#" => &["cs"],
            "powershell" => &["ps1", "psm1"],
            "batch" => &["bat", "cmd"],
            "zsh" => &["zsh"],
            "fish" => &["fish"],
            "dockerfile" => {
                globs.push("**/Dockerfile".to_string());
                globs.push("**/*.dockerfile".to_string());
                extensions
            }
            "makefile" => {
                globs.push("**/Makefile".to_string());
                globs.push("**/*.mk".to_string());
                extensions
            }
            // Most types are named after their extension (lua, nix, zig, vue, svelte...)
            other => {
                globs.push(format!("**/*.{other}"));
                extensions
            }
        };
        for ext in exts {
            let glob = format!("**/*.{ext}");
            if !globs.contains(&glob) {
                globs.push(glob);
            }
        }
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

/// A hook of a repository of the pre-commit ecosystem: `from` + `rev` + `id`, its definition
/// is read from the repository when the config loads. Only what the config changes is kept.
fn repository_hook(
    url: &str,
    rev: Option<&str>,
    id: &str,
    hook: &Value,
    t: Trigger,
    global: &Global,
) -> Hook {
    let (include, include_regex) = files_of(hook, global);
    let own_exclude = hook
        .get("exclude")
        .and_then(Value::as_str)
        .filter(|r| !r.is_empty() && *r != "^$")
        .map(String::from);
    let (exclude, exclude_regex) = exclude_of(either(own_exclude, global.exclude.clone()));
    let list = |key: &str| hook.get(key).map(|v| strings(Some(v)));
    Hook {
        name: hook
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or(id)
            .to_string(),
        trigger: Some(t),
        from: Some(url.to_string()),
        rev: rev.map(String::from),
        id: Some(id.to_string()),
        args: list("args"),
        additional_dependencies: list("additional_dependencies").filter(|d| !d.is_empty()),
        include,
        include_regex,
        exclude,
        exclude_regex,
        always_run: hook
            .get("always_run")
            .and_then(Value::as_bool)
            .filter(|a| *a),
        types: Some(kinds(&strings(hook.get("types")))).filter(|k| !k.is_empty()),
        ..Default::default()
    }
}

/// `types` / `types_or` / `files` of a hook (with the top-level `files`)
fn files_of(hook: &Value, global: &Global) -> (Option<Vec<String>>, Option<String>) {
    let mut include = types_globs(&strings(hook.get("types")));
    include.extend(types_globs(&strings(hook.get("types_or"))));
    let mut include_regex = None;
    let files = hook
        .get("files")
        .and_then(Value::as_str)
        .filter(|r| !r.is_empty())
        .map(String::from)
        .or_else(|| global.files.clone());
    if let Some(files) = files {
        match regex_globs(&files) {
            Some(globs) if include.is_empty() => include = globs,
            _ => include_regex = Some(files),
        }
    }
    ((!include.is_empty()).then_some(include), include_regex)
}

/// `exclude` as globs when it is simple, else as a regex
fn exclude_of(regex: Option<String>) -> (Option<Vec<String>>, Option<String>) {
    match regex {
        Some(regex) => match regex_globs(&regex) {
            Some(globs) => (Some(globs), None),
            None => (None, Some(regex)),
        },
        None => (None, None),
    }
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
    rev: Option<&str>,
    id: &str,
    hook: &Value,
    t: Trigger,
    global: &Global,
) -> Option<Hook> {
    if url == "meta" {
        imported.note(format!(
            "pre-commit: `{id}` checks the pre-commit config itself, it was skipped"
        ));
        return None;
    }
    if url != "local" {
        return Some(repository_hook(url, rev, id, hook, t, global));
    }
    let args = strings(hook.get("args")).join(" ");
    let mut tool = None;
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
        if language == "pygrep" {
            let exe = std::env::current_exe()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|_| "hooker".into());
            let cmd = format!(
                "'{}' pygrep {args} -- '{}'",
                exe.replace('\'', r"'\''"),
                entry.replace('\'', r"'\''")
            );
            (cmd, pass_filenames, false)
        } else {
            // A tool of another language: the installed one, else an environment with its
            // `additional_dependencies` (only when read as it is: a hooker.toml has no field
            // for it)
            let dependencies = strings(hook.get("additional_dependencies"));
            if !["system", "script", "unsupported", "unsupported_script"].contains(&language) {
                tool = Some(crate::tools::Tool {
                    language: language.to_string(),
                    repo: None,
                    source: format!("local:{id}"),
                    dependencies: dependencies.clone(),
                    isolate: None,
                    program: entry
                        .split_whitespace()
                        .next()
                        .unwrap_or_default()
                        .to_string(),
                });
                if !dependencies.is_empty() {
                    imported.note(format!(
                        "pre-commit: `{id}` needs {} in its {language} environment: it is built when the config is read as it is, install them yourself with a hooker.toml",
                        dependencies.join(", ")
                    ));
                }
            }
            let cmd = if args.is_empty() {
                entry.to_string()
            } else {
                format!("{entry} {args}")
            };
            (cmd, pass_filenames, false)
        }
    } else {
        return None;
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
        types: Some(kinds(&strings(hook.get("types")))).filter(|k| !k.is_empty()),
        cmd: Some(cmd),
        stage_fixed: stage_fixed.then_some(true),
        tool,
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

        // Hooks of repositories: `from` + `rev` + `id`, only what the config changes
        let large = find("check-added-large-files");
        assert_eq!(
            large.from.as_deref(),
            Some("https://github.com/pre-commit/pre-commit-hooks")
        );
        assert_eq!(large.rev.as_deref(), Some("v4.6.0"));
        assert_eq!(large.id.as_deref(), Some("check-added-large-files"));
        assert_eq!(large.args, Some(vec!["--maxkb=200".to_string()]));
        assert!(large.cmd.is_none() && large.trigger.is_none());
        let ruff = find("ruff");
        assert_eq!(ruff.args, Some(vec!["--fix".to_string()]));
        assert!(find("check-yaml").args.is_none());
        let tests = find("tests");
        assert_eq!(tests.trigger, Some(Trigger::PrePush));
        assert_eq!(tests.cmd.as_deref(), Some("pytest -q"));
        assert_eq!(
            find("mypy").include,
            Some(vec!["**/*.py".into(), "**/*.pyi".into()])
        );
        // Its stage (commit-msg) comes from its definition
        assert!(find("conventional-pre-commit").trigger.is_none());
        for hook in &hooks {
            hook.validate().unwrap();
        }
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
