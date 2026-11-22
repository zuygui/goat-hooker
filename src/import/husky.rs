//! husky: `.husky/<git hook>` scripts (husky 5+), or `husky.hooks` in package.json (husky 4).
//! A `lint-staged` line is replaced by the lint-staged config, one hook per command.

use std::path::Path;

use serde_json::Value;

use super::{Imported, globs, name_from_command, read_json, read_yaml, trigger};
use crate::{
    config::{Hook, Trigger},
    errors::GoatHookerError,
};

pub fn import(root: &Path, imported: &mut Imported) -> Result<(), GoatHookerError> {
    let package = root.join("package.json");
    let package_json = if package.is_file() {
        read_json(&package).ok()
    } else {
        None
    };

    // husky 5+: one script per Git hook
    let dir = root.join(".husky");
    if dir.is_dir() {
        let mut found = false;
        for t in Trigger::ALL {
            let file = dir.join(t.as_str());
            if let Ok(script) = std::fs::read_to_string(&file) {
                imported.source(root, &file);
                import_script(root, imported, t, &script, package_json.as_ref())?;
                found = true;
            }
        }
        if found && let Some(path) = crate::git::hooks_path_setting(root) {
            imported.note(format!(
                "Git runs the hooks of {path} (core.hooksPath, set by husky): run `git config --unset core.hooksPath` before `hooker install`"
            ));
        }
    }

    // husky 4: `"husky": { "hooks": { "pre-commit": "lint-staged" } }`
    if let Some(hooks) = package_json
        .as_ref()
        .and_then(|p| p.pointer("/husky/hooks"))
        .and_then(Value::as_object)
    {
        imported.source(root, &package);
        for (name, cmd) in hooks {
            let (Some(t), Some(cmd)) = (trigger(name), cmd.as_str()) else {
                imported.note(format!("husky: unknown Git hook `{name}`, skipped"));
                continue;
            };
            // husky 4 gave the hook arguments in HUSKY_GIT_PARAMS
            let cmd = cmd
                .replace("-E HUSKY_GIT_PARAMS", "--edit \"$1\"")
                .replace("$HUSKY_GIT_PARAMS", "\"$@\"");
            import_script(root, imported, t, &cmd, package_json.as_ref())?;
        }
    }
    Ok(())
}

fn import_script(
    root: &Path,
    imported: &mut Imported,
    t: Trigger,
    script: &str,
    package_json: Option<&Value>,
) -> Result<(), GoatHookerError> {
    let mut rest = Vec::new();
    for line in script.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty()
            || trimmed.starts_with('#')
            || trimmed.contains("husky.sh")
            || trimmed.starts_with("export ")
        {
            continue;
        }
        if let Some(runner) = lint_staged_runner(trimmed) {
            lint_staged(root, imported, t, runner, package_json)?;
        } else {
            rest.push(line.to_string());
        }
    }

    // What's left of the script stays a script
    if !rest.is_empty() {
        let cmd = rest.join("\n");
        let name = if rest.len() == 1 {
            name_from_command(&cmd)
        } else {
            format!("husky {}", t.as_str())
        };
        imported.add(
            Path::new(""),
            Hook {
                name,
                trigger: Some(t),
                cmd: Some(cmd),
                ..Default::default()
            },
        );
    }
    Ok(())
}

/// `npx lint-staged` → `npx` (the command running the local binaries)
fn lint_staged_runner(line: &str) -> Option<&'static str> {
    let words: Vec<&str> = line
        .split_whitespace()
        .filter(|w| !w.starts_with('-'))
        .collect();
    let position = words.iter().position(|w| *w == "lint-staged")?;
    Some(match words.get(..position)? {
        ["pnpm", ..] => "pnpm exec",
        ["yarn", ..] => "yarn",
        ["bunx"] | ["bun", ..] => "bunx",
        _ => "npx",
    })
}

/// lint-staged config: `{ "*.{js,ts}": ["eslint --fix", "prettier --write"] }`
fn lint_staged(
    root: &Path,
    imported: &mut Imported,
    t: Trigger,
    runner: &str,
    package_json: Option<&Value>,
) -> Result<(), GoatHookerError> {
    let mut config = package_json.and_then(|p| p.get("lint-staged")).cloned();
    if config.is_some() {
        imported.source(root, &root.join("package.json"));
    }
    for file in [".lintstagedrc", ".lintstagedrc.json"] {
        let path = root.join(file);
        if config.is_none() && path.is_file() {
            config = read_json(&path).or_else(|_| read_yaml(&path)).ok();
            imported.source(root, &path);
        }
    }
    for file in [".lintstagedrc.yaml", ".lintstagedrc.yml"] {
        let path = root.join(file);
        if config.is_none() && path.is_file() {
            config = read_yaml(&path).ok();
            imported.source(root, &path);
        }
    }
    let Some(Value::Object(tasks)) = config else {
        imported.note(
            "lint-staged: no JSON or YAML config found (a JavaScript config can't be imported), add its commands by hand",
        );
        return Ok(());
    };

    for (pattern, commands) in tasks {
        let commands = match commands {
            Value::String(cmd) => vec![cmd],
            Value::Array(items) => items
                .iter()
                .filter_map(|i| i.as_str().map(str::to_string))
                .collect(),
            _ => {
                imported.note(format!("lint-staged: `{pattern}` isn't a command, skipped"));
                continue;
            }
        };
        // The commands of a pattern run one after the other
        let mut previous: Option<String> = None;
        for command in commands {
            let first = command.split_whitespace().next().unwrap_or_default();
            let local_binary = ![
                "npx", "pnpm", "yarn", "bunx", "bun", "npm", "node", "sh", "bash", "git",
            ]
            .contains(&first)
                && !first.contains('/');
            let cmd = if local_binary {
                format!("{runner} {command} {{files}}")
            } else {
                format!("{command} {{files}}")
            };
            let name = imported.add(
                Path::new(""),
                Hook {
                    name: name_from_command(&command),
                    trigger: Some(t),
                    include: Some(globs(&pattern)),
                    cmd: Some(cmd),
                    // lint-staged adds the files changed by the commands
                    stage_fixed: Some(true),
                    needs: previous.take().map(|p| vec![p]),
                    ..Default::default()
                },
            );
            previous = Some(name);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn husky_with_lint_staged() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join(".husky/_")).unwrap();
        fs::write(
            root.join(".husky/pre-commit"),
            "#!/usr/bin/env sh\n. \"$(dirname -- \"$0\")/_/husky.sh\"\n\npnpm lint-staged\n",
        )
        .unwrap();
        fs::write(
            root.join(".husky/commit-msg"),
            "npx --no -- commitlint --edit $1\n",
        )
        .unwrap();
        fs::write(
            root.join("package.json"),
            r#"{ "lint-staged": { "*.{js,ts}": ["eslint --fix", "prettier --write"], "*.md": "prettier --write" } }"#,
        )
        .unwrap();

        let mut imported = Imported::default();
        import(root, &mut imported).unwrap();
        let hooks = imported.configs[Path::new("")].hooks.clone().unwrap();
        let summary: Vec<(String, String, Option<Vec<String>>)> = hooks
            .iter()
            .map(|h| (h.name.clone(), h.cmd.clone().unwrap(), h.needs.clone()))
            .collect();
        assert_eq!(
            summary,
            vec![
                (
                    "eslint".into(),
                    "pnpm exec eslint --fix {files}".into(),
                    None
                ),
                (
                    "prettier".into(),
                    "pnpm exec prettier --write {files}".into(),
                    Some(vec!["eslint".into()])
                ),
                (
                    "prettier 2".into(),
                    "pnpm exec prettier --write {files}".into(),
                    None
                ),
                (
                    "commitlint".into(),
                    "npx --no -- commitlint --edit $1".into(),
                    None
                ),
            ]
        );
        assert_eq!(
            hooks[0].include,
            Some(vec!["**/*.js".into(), "**/*.ts".into()])
        );
        assert_eq!(hooks[3].trigger, Some(Trigger::CommitMsg));
    }

    #[test]
    fn husky_4_in_package_json() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("package.json"),
            r#"{ "husky": { "hooks": { "pre-push": "npm test", "commit-msg": "commitlint -E HUSKY_GIT_PARAMS" } } }"#,
        )
        .unwrap();
        let mut imported = Imported::default();
        import(dir.path(), &mut imported).unwrap();
        let hooks = imported.configs[Path::new("")].hooks.clone().unwrap();
        assert_eq!(hooks.len(), 2);
        assert!(
            hooks.iter().any(
                |h| h.trigger == Some(Trigger::PrePush) && h.cmd.as_deref() == Some("npm test")
            )
        );
    }
}
