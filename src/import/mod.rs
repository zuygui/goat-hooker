//! `hooker import`: converts the configs of other Git hooks managers to `hooker.config.json`.
//!
//! - husky (`.husky/*` scripts, or `husky.hooks` in package.json) and lint-staged
//! - lefthook (`lefthook.yml`)
//! - pre-commit (`.pre-commit-config.yaml`)
//!
//! What can't be converted is reported in `notes`, the generated files are always valid configs.

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use serde_json::Value;

use crate::{
    config::{Config, HOOK_CONFIG_FILENAME, Hook, SCHEMA_URL, Trigger},
    errors::GoatHookerError,
};

mod husky;
mod lefthook;
mod precommit;

/// Result of an import
#[derive(Debug, Default)]
pub struct Imported {
    /// Configs to write, by folder relative to the repository root (`""` for the root)
    pub configs: BTreeMap<PathBuf, Config>,
    /// Files that were converted
    pub sources: Vec<PathBuf>,
    /// What couldn't be converted exactly
    pub notes: Vec<String>,
}

impl Imported {
    pub fn is_empty(&self) -> bool {
        self.configs
            .values()
            .all(|c| c.hooks.as_ref().is_none_or(|h| h.is_empty()))
    }

    pub fn hooks_count(&self) -> usize {
        self.configs
            .values()
            .map(|c| c.hooks.as_ref().map_or(0, Vec::len))
            .sum()
    }

    /// Adds a hook to the config of `dir`, with a unique name
    pub(crate) fn add(&mut self, dir: &Path, mut hook: Hook) -> String {
        let config = self.configs.entry(dir.to_path_buf()).or_default();
        let hooks = config.hooks.get_or_insert_with(Vec::new);
        let base = hook.name.clone();
        let mut n = 2;
        while hooks.iter().any(|h| h.name == hook.name) {
            hook.name = format!("{base} {n}");
            n += 1;
        }
        let name = hook.name.clone();
        hooks.push(hook);
        name
    }

    pub(crate) fn note(&mut self, note: impl Into<String>) {
        self.notes.push(note.into());
    }

    pub(crate) fn source(&mut self, root: &Path, file: &Path) {
        let file = file.strip_prefix(root).unwrap_or(file).to_path_buf();
        if !self.sources.contains(&file) {
            self.sources.push(file);
        }
    }
}

/// Reads the configs of the other hooks managers found at the root of the repository
pub fn detect(root: &Path) -> Result<Imported, GoatHookerError> {
    let mut imported = Imported::default();
    husky::import(root, &mut imported)?;
    lefthook::import(root, &mut imported)?;
    precommit::import(root, &mut imported)?;

    // Root config first, with the sub-projects
    let sub_projects: Vec<String> = imported
        .configs
        .keys()
        .filter(|dir| !dir.as_os_str().is_empty())
        .map(|dir| dir.to_string_lossy().replace('\\', "/"))
        .collect();
    let root_config = imported.configs.entry(PathBuf::new()).or_default();
    if !sub_projects.is_empty() {
        root_config.imports = Some(sub_projects);
    }
    for config in imported.configs.values_mut() {
        config.schema = Some(SCHEMA_URL.to_string());
        config.hooks.get_or_insert_with(Vec::new);
    }
    Ok(imported)
}

/// Writes the configs, refuses to replace an existing one without `force`
pub fn write(
    root: &Path,
    imported: &Imported,
    force: bool,
) -> Result<Vec<PathBuf>, GoatHookerError> {
    let files: Vec<PathBuf> = imported
        .configs
        .keys()
        .map(|dir| root.join(dir).join(HOOK_CONFIG_FILENAME))
        .collect();
    if !force && let Some(existing) = files.iter().find(|f| f.exists()) {
        return Err(GoatHookerError::InvalidHook {
            name: "import".to_string(),
            reason: format!(
                "{} already exists, use --force to replace it",
                existing.strip_prefix(root).unwrap_or(existing).display()
            ),
        });
    }
    for (dir, config) in &imported.configs {
        let dir = root.join(dir);
        fs::create_dir_all(&dir)?;
        config.write_to_file(&dir.join(HOOK_CONFIG_FILENAME))?;
    }
    Ok(files)
}

// MARK: helpers shared by the importers

pub(crate) fn trigger(name: &str) -> Option<Trigger> {
    Trigger::ALL.iter().copied().find(|t| t.as_str() == name)
}

/// `*.{js,ts}` → `*.js`, `*.ts`
pub(crate) fn expand_braces(pattern: &str) -> Vec<String> {
    let Some(start) = pattern.find('{') else {
        return vec![pattern.to_string()];
    };
    let Some(len) = pattern[start..].find('}') else {
        return vec![pattern.to_string()];
    };
    let end = start + len;
    let (before, after) = (&pattern[..start], &pattern[end + 1..]);
    pattern[start + 1..end]
        .split(',')
        .flat_map(|choice| expand_braces(&format!("{before}{choice}{after}")))
        .collect()
}

/// Glob of another tool → globs of goat-hooker (a pattern without `/` matches at any depth)
pub(crate) fn globs(pattern: &str) -> Vec<String> {
    expand_braces(pattern.trim())
        .into_iter()
        .map(|p| {
            let p = p.trim_start_matches("./").to_string();
            if p.contains('/') || p.starts_with("**") {
                p
            } else {
                format!("**/{p}")
            }
        })
        .collect()
}

/// A string or a list of strings
pub(crate) fn strings(value: Option<&Value>) -> Vec<String> {
    match value {
        Some(Value::String(s)) => vec![s.clone()],
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(|i| i.as_str().map(str::to_string))
            .collect(),
        _ => vec![],
    }
}

pub(crate) fn read_yaml(path: &Path) -> Result<Value, GoatHookerError> {
    let content = fs::read_to_string(path)?;
    serde_yaml_ng::from_str(&content).map_err(|err| GoatHookerError::InvalidHook {
        name: path.display().to_string(),
        reason: format!("invalid YAML: {err}"),
    })
}

pub(crate) fn read_json(path: &Path) -> Result<Value, GoatHookerError> {
    let content = fs::read_to_string(path)?;
    serde_json::from_str(&content).map_err(|source| GoatHookerError::ParseError {
        path: path.to_path_buf(),
        source,
    })
}

/// Hook name from a command: `npx eslint --fix` → `eslint`
pub(crate) fn name_from_command(cmd: &str) -> String {
    const RUNNERS: [&str; 9] = [
        "npx", "pnpm", "exec", "yarn", "bunx", "bun", "npm", "run", "--no",
    ];
    cmd.split_whitespace()
        .filter(|w| !w.starts_with('-') || *w == "--")
        .find(|w| !RUNNERS.contains(w) && *w != "--" && !w.contains('='))
        .map(|w| w.rsplit('/').next().unwrap_or(w).to_string())
        .unwrap_or_else(|| "script".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn braces_and_globs() {
        assert_eq!(expand_braces("*.{js,ts}"), vec!["*.js", "*.ts"]);
        assert_eq!(
            expand_braces("{src,lib}/*.{c,h}"),
            vec!["src/*.c", "src/*.h", "lib/*.c", "lib/*.h"]
        );
        assert_eq!(globs("*.{js,ts}"), vec!["**/*.js", "**/*.ts"]);
        assert_eq!(globs("./src/**/*.rs"), vec!["src/**/*.rs"]);
    }

    #[test]
    fn names() {
        assert_eq!(name_from_command("npx eslint --fix"), "eslint");
        assert_eq!(name_from_command("pnpm exec prettier --write"), "prettier");
        assert_eq!(
            name_from_command("npx --no -- commitlint --edit $1"),
            "commitlint"
        );
        assert_eq!(name_from_command("./scripts/check.sh"), "check.sh");
    }
}
