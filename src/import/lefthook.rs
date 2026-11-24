//! lefthook: `lefthook.yml` (commands, jobs and scripts of each Git hook).
//! A command with `root` goes to the config of that folder (a sub-project).

use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

use super::{Imported, globs, read_json, read_yaml, strings, trigger};
use crate::{config::Hook, errors::GoatHookerError};

pub(crate) const FILES: [&str; 7] = [
    "lefthook.yml",
    "lefthook.yaml",
    ".lefthook.yml",
    ".lefthook.yaml",
    "lefthook.json",
    ".config/lefthook.yml",
    ".config/lefthook.yaml",
];

/// Keys of lefthook that goat-hooker has no equivalent for
const UNSUPPORTED: [&str; 4] = ["tags", "files", "interactive", "use_stdin"];

pub fn import(root: &Path, imported: &mut Imported) -> Result<(), GoatHookerError> {
    let Some(file) = FILES.iter().map(|f| root.join(f)).find(|f| f.is_file()) else {
        return Ok(());
    };
    imported.source(root, &file);
    let config = if file.extension().is_some_and(|e| e == "json") {
        read_json(&file)?
    } else {
        read_yaml(&file)?
    };
    let Value::Object(mut config) = config else {
        return Ok(());
    };
    // `lefthook-local.yml`: changes of this computer, merged over the config
    for local in [
        "lefthook-local.yml",
        "lefthook-local.yaml",
        ".lefthook-local.yml",
    ] {
        let file = root.join(local);
        if file.is_file()
            && let Ok(Value::Object(local)) = read_yaml(&file)
        {
            merge(&mut config, local);
        }
    }
    for key in ["extends", "remotes", "remote"] {
        if config.contains_key(key) {
            imported.note(format!(
                "lefthook: `{key}` wasn't imported, use `from` / `path` hooks to share hooks"
            ));
        }
    }

    for (name, hook_config) in &config {
        let Some(t) = trigger(name) else {
            continue;
        };
        let Value::Object(hook_config) = hook_config else {
            continue;
        };
        if hook_config.get("parallel").and_then(Value::as_bool) == Some(true) {
            imported.configs.entry(PathBuf::new()).or_default().parallel = Some(true);
        }
        let piped = hook_config.get("piped").and_then(Value::as_bool) == Some(true);
        let mut previous: Option<(PathBuf, String)> = None;
        let mut add = |imported: &mut Imported, job_name: &str, job: &Map<String, Value>| {
            if let Some((dir, hook)) = convert(imported, t.as_str(), job_name, job) {
                let mut hook = hook;
                // `piped`: each command waits for the previous one
                if piped && let Some((previous_dir, previous_name)) = previous.as_ref() {
                    let reference = if *previous_dir == dir {
                        previous_name.clone()
                    } else {
                        format!("{}:{previous_name}", project(previous_dir))
                    };
                    hook.needs = Some(vec![reference]);
                }
                let name = imported.add(&dir, hook);
                previous = Some((dir, name));
            }
        };

        if let Some(Value::Object(commands)) = hook_config.get("commands") {
            for (job_name, job) in commands {
                if let Value::Object(job) = job {
                    add(imported, job_name, job);
                }
            }
        }
        if let Some(Value::Array(jobs)) = hook_config.get("jobs") {
            for (i, job) in jobs.iter().enumerate() {
                let Value::Object(job) = job else { continue };
                if job.contains_key("group") {
                    imported.note(format!(
                        "lefthook: the job groups of {} weren't imported",
                        t.as_str()
                    ));
                    continue;
                }
                let job_name = job
                    .get("name")
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .unwrap_or_else(|| format!("job {}", i + 1));
                add(imported, &job_name, job);
            }
        }
        if let Some(Value::Object(scripts)) = hook_config.get("scripts") {
            for (script, job) in scripts {
                let Value::Object(job) = job else { continue };
                let mut job = job.clone();
                let runner = job
                    .get("runner")
                    .and_then(Value::as_str)
                    .unwrap_or("sh")
                    .to_string();
                job.insert(
                    "run".into(),
                    Value::String(format!("{runner} .lefthook/{}/{script} \"$@\"", t.as_str())),
                );
                add(imported, script, &job);
            }
        }
    }
    Ok(())
}

/// Deep merge of `lefthook-local.yml` over `lefthook.yml` (objects merged, the rest replaced)
fn merge(base: &mut Map<String, Value>, over: Map<String, Value>) {
    for (key, value) in over {
        match (base.get_mut(&key), value) {
            (Some(Value::Object(base)), Value::Object(over)) => merge(base, over),
            (_, value) => {
                base.insert(key, value);
            }
        }
    }
}

/// `skip` / `only` of lefthook: `true`, or conditions (`merge`, `rebase`, `ref: main`).
/// Returns the branches the condition names, `None` when it isn't only about branches.
fn branch_condition(value: &Value) -> Option<Vec<String>> {
    let items = match value {
        Value::Array(items) => items.clone(),
        other => vec![other.clone()],
    };
    let mut branches = Vec::new();
    for item in items {
        let branch = item.get("ref").and_then(Value::as_str)?;
        branches.push(branch.to_string());
    }
    (!branches.is_empty()).then_some(branches)
}

/// Name of a sub-project in references (`web:lint`)
fn project(dir: &Path) -> String {
    if dir.as_os_str().is_empty() {
        "root".to_string()
    } else {
        dir.to_string_lossy().replace('\\', "/")
    }
}

/// A command or a job → (folder of its config, hook)
fn convert(
    imported: &mut Imported,
    trigger_name: &str,
    name: &str,
    job: &Map<String, Value>,
) -> Option<(PathBuf, Hook)> {
    let Some(run) = job.get("run").and_then(Value::as_str) else {
        imported.note(format!(
            "lefthook: `{name}` ({trigger_name}) has no `run`, skipped"
        ));
        return None;
    };
    for key in UNSUPPORTED {
        if job.contains_key(key) {
            imported.note(format!(
                "lefthook: `{key}` of `{name}` ({trigger_name}) has no equivalent, it was ignored"
            ));
        }
    }
    // `skip: true` turns the command off; branches become `skip_branches` / `branches`
    let mut branches = None;
    let mut skip_branches = None;
    for (key, target) in [("skip", &mut skip_branches), ("only", &mut branches)] {
        let Some(value) = job.get(key) else { continue };
        if key == "skip" && value.as_bool() == Some(true) {
            return None;
        }
        match branch_condition(value) {
            Some(names) => *target = Some(names),
            None if value.as_bool() == Some(false) => {}
            None => imported.note(format!(
                "lefthook: `{key}` of `{name}` ({trigger_name}) only converts `ref:` conditions, the others were ignored"
            )),
        }
    }

    let dir = job
        .get("root")
        .and_then(Value::as_str)
        .map(|r| r.trim_matches('/').trim_start_matches("./"))
        .filter(|r| !r.is_empty())
        .map(PathBuf::from)
        .unwrap_or_default();
    // Patterns relative to the folder of the config
    let prefix = format!("{}/", dir.to_string_lossy());
    let relative = |pattern: String| {
        pattern
            .strip_prefix(&prefix)
            .map(str::to_string)
            .unwrap_or(pattern)
    };

    let include: Vec<String> = strings(job.get("glob"))
        .iter()
        .flat_map(|g| globs(g))
        .map(relative)
        .collect();
    let mut exclude_regex = None;
    let exclude: Vec<String> = match job.get("exclude") {
        // A regex on the path from the root of the repository
        Some(Value::String(regex)) if dir.as_os_str().is_empty() => {
            exclude_regex = Some(regex.clone());
            vec![]
        }
        Some(Value::String(regex)) => {
            imported.note(format!(
                "lefthook: the `exclude` regex of `{name}` ({regex}) is on paths from the repository root, `root` changes them: it was ignored"
            ));
            vec![]
        }
        value => strings(value)
            .iter()
            .flat_map(|g| globs(g))
            .map(relative)
            .collect(),
    };

    let mut cmd = run
        .replace("{staged_files}", "{files}")
        .replace("{push_files}", "{files}")
        .replace("{all_files}", "{files}")
        .replace("{lefthook_job_name}", name)
        .replace("{0}", "\"$@\"");
    for i in 1..=3 {
        cmd = cmd.replace(&format!("{{{i}}}"), &format!("\"${i}\""));
    }
    // `env`: variables of the command
    if let Some(Value::Object(env)) = job.get("env") {
        let exports: Vec<String> = env
            .iter()
            .filter_map(|(k, v)| {
                let value = match v {
                    Value::String(s) => s.clone(),
                    other => other.to_string(),
                };
                k.chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_')
                    .then(|| format!("export {k}='{}'", value.replace('\'', r"'\''")))
            })
            .collect();
        if !exports.is_empty() {
            cmd = format!("{}; {cmd}", exports.join("; "));
        }
    }
    // `fail_text`: shown when the command fails
    if let Some(text) = job.get("fail_text").and_then(Value::as_str) {
        cmd = format!(
            "{{ {cmd}\n}} || {{ echo '{}'; exit 1; }}",
            text.replace('\'', r"'\''")
        );
    }
    if run.contains("{all_files}") {
        imported.note(format!(
            "lefthook: `{name}` used {{all_files}}, it now gets the staged (or pushed) files"
        ));
    }

    Some((
        dir,
        Hook {
            name: name.to_string(),
            trigger: super::trigger(trigger_name),
            include: (!include.is_empty()).then_some(include),
            exclude: (!exclude.is_empty()).then_some(exclude),
            exclude_regex,
            branches,
            skip_branches,
            cmd: Some(cmd),
            stage_fixed: job
                .get("stage_fixed")
                .and_then(Value::as_bool)
                .filter(|s| *s),
            ..Default::default()
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Trigger;
    use std::fs;

    #[test]
    fn lefthook_commands_jobs_and_roots() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("lefthook.yml"),
            r#"
pre-commit:
  parallel: true
  commands:
    eslint:
      root: "web/"
      glob: "*.{js,ts}"
      run: npx eslint --fix {staged_files}
      stage_fixed: true
    rustfmt:
      glob: "*.rs"
      exclude: "generated/.*"
      run: rustfmt {staged_files}
commit-msg:
  jobs:
    - name: commitlint
      run: npx commitlint --edit {1}
pre-push:
  piped: true
  commands:
    build:
      run: cargo build
    test:
      run: cargo test
"#,
        )
        .unwrap();

        let mut imported = Imported::default();
        import(dir.path(), &mut imported).unwrap();

        let root = &imported.configs[Path::new("")];
        assert_eq!(root.parallel, Some(true));
        let web = imported.configs[Path::new("web")].hooks.clone().unwrap();
        assert_eq!(web[0].cmd.as_deref(), Some("npx eslint --fix {files}"));
        assert_eq!(
            web[0].include,
            Some(vec!["**/*.js".into(), "**/*.ts".into()])
        );
        assert_eq!(web[0].stage_fixed, Some(true));

        let hooks = root.hooks.clone().unwrap();
        let rustfmt = hooks.iter().find(|h| h.name == "rustfmt").unwrap();
        assert!(rustfmt.exclude.is_none());
        assert_eq!(rustfmt.exclude_regex.as_deref(), Some("generated/.*"));
        let commitlint = hooks.iter().find(|h| h.name == "commitlint").unwrap();
        assert_eq!(commitlint.trigger, Some(Trigger::CommitMsg));
        assert_eq!(
            commitlint.cmd.as_deref(),
            Some("npx commitlint --edit \"$1\"")
        );
        let test = hooks.iter().find(|h| h.name == "test").unwrap();
        assert_eq!(test.needs, Some(vec!["build".to_string()]));
    }

    #[test]
    fn lefthook_local_skip_only_env_and_fail_text() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("lefthook.yml"),
            r#"
pre-commit:
  commands:
    lint:
      run: eslint {staged_files}
      env:
        NODE_ENV: test
      fail_text: "Run npm run lint:fix"
    slow:
      run: cargo test
      only:
        - ref: main
    off:
      run: echo off
    docs:
      run: echo docs
      skip:
        - ref: release
"#,
        )
        .unwrap();
        fs::write(
            dir.path().join("lefthook-local.yml"),
            "pre-commit:\n  commands:\n    off:\n      skip: true\n",
        )
        .unwrap();

        let mut imported = Imported::default();
        import(dir.path(), &mut imported).unwrap();
        let hooks = imported.configs[Path::new("")].hooks.clone().unwrap();
        let names: Vec<&str> = hooks.iter().map(|h| h.name.as_str()).collect();
        assert_eq!(names, ["lint", "slow", "docs"]);
        assert_eq!(
            hooks[0].cmd.as_deref(),
            Some(
                "{ export NODE_ENV='test'; eslint {files}\n} || { echo 'Run npm run lint:fix'; exit 1; }"
            )
        );
        assert_eq!(hooks[1].branches, Some(vec!["main".to_string()]));
        assert_eq!(hooks[2].skip_branches, Some(vec!["release".to_string()]));
    }
}
