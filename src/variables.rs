//! Variables shared between hooks.
//!
//! A config declares them (`variables`), with who may write and read them (`writers`, `readers`).
//! A hook declares the ones it `writes` (with `echo "name=value" >> $HOOKER_VARS`) and `reads`
//! (as `$name`): it runs after the hooks of the same trigger that write them.
//! The values are kept between runs in `.git/goat-hooker/variables.json`.

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use crate::{
    config::{Config, Variable},
    errors::GoatHookerError,
    git, graph,
};

/// A declared variable
#[derive(Debug, Clone)]
pub struct Declared {
    pub name: String,
    pub variable: Variable,
    /// Project declaring it (`root` or folder)
    pub project: String,
    pub file: Option<PathBuf>,
}

/// How a variable is used, for `hooker list` and the app
#[derive(Debug, Clone)]
pub struct Usage {
    pub declared: Declared,
    /// Current value (stored, or default)
    pub value: Option<String>,
    /// Hooks declaring they write / read it (`project:hook`)
    pub writers: Vec<String>,
    pub readers: Vec<String>,
    /// Hooks using it without being allowed to
    pub violations: Vec<String>,
}

fn error(name: &str, reason: String) -> GoatHookerError {
    GoatHookerError::InvalidHook {
        name: name.to_string(),
        reason,
    }
}

/// Variables declared in all the configs (a name can be declared only once)
pub fn declared(configs: &[Config]) -> Result<BTreeMap<String, Declared>, GoatHookerError> {
    let root = root_dir(configs);
    let mut declared: BTreeMap<String, Declared> = BTreeMap::new();
    for config in configs {
        let project = graph::project_name(&root, config.base_dir.as_deref().unwrap_or(&root));
        for (name, variable) in config.variables.iter().flatten() {
            if let Some(existing) = declared.get(name) {
                return Err(error(
                    name,
                    format!(
                        "variable declared twice (in {} and {project})",
                        existing.project
                    ),
                ));
            }
            declared.insert(
                name.clone(),
                Declared {
                    name: name.clone(),
                    variable: variable.clone(),
                    project: project.clone(),
                    file: config.file.clone(),
                },
            );
        }
    }
    Ok(declared)
}

fn root_dir(configs: &[Config]) -> PathBuf {
    configs
        .first()
        .and_then(|c| c.base_dir.clone())
        .unwrap_or_default()
}

/// Is `project:hook` in a `writers` / `readers` list? (no list: everyone)
pub fn allowed(
    list: Option<&Vec<String>>,
    declaring_project: &str,
    project: &str,
    hook: &str,
) -> bool {
    let Some(list) = list else {
        return true;
    };
    list.iter().any(|entry| {
        if entry == "*" {
            return true;
        }
        match entry.rsplit_once(':') {
            Some((p, h)) => {
                let p = match p.trim_matches('/') {
                    "" | "." | "root" => "root",
                    other => other.trim_start_matches("./"),
                };
                p == project && (h == hook || h == "*")
            }
            None => declaring_project == project && entry == hook,
        }
    })
}

/// Checks that each hook only uses declared variables it is allowed to use
pub fn check(configs: &[Config]) -> Result<BTreeMap<String, Declared>, GoatHookerError> {
    let declared = declared(configs)?;
    for usage in usages(configs, &declared) {
        if let Some(violation) = usage.violations.first() {
            return Err(error(&usage.declared.name, violation.clone()));
        }
    }
    let root = root_dir(configs);
    for config in configs {
        let project = graph::project_name(&root, config.base_dir.as_deref().unwrap_or(&root));
        for hook in config.hooks.iter().flatten() {
            for name in hook.reads.iter().chain(hook.writes.iter()).flatten() {
                if !declared.contains_key(name) {
                    return Err(error(
                        &format!("{project}:{}", hook.name),
                        format!("variable `{name}` isn't declared in `variables`"),
                    ));
                }
                if hook.inputs.as_ref().is_some_and(|i| i.contains_key(name)) {
                    return Err(error(
                        &format!("{project}:{}", hook.name),
                        format!("`{name}` is both an input and a variable"),
                    ));
                }
            }
        }
    }
    Ok(declared)
}

fn usages(configs: &[Config], declared: &BTreeMap<String, Declared>) -> Vec<Usage> {
    let root = root_dir(configs);
    let mut usages: BTreeMap<String, Usage> = declared
        .iter()
        .map(|(name, d)| {
            (
                name.clone(),
                Usage {
                    declared: d.clone(),
                    value: d.variable.default.clone(),
                    writers: vec![],
                    readers: vec![],
                    violations: vec![],
                },
            )
        })
        .collect();
    for config in configs {
        let project = graph::project_name(&root, config.base_dir.as_deref().unwrap_or(&root));
        for hook in config.hooks.iter().flatten() {
            let reference = format!("{project}:{}", hook.name);
            for (names, writing) in [(&hook.writes, true), (&hook.reads, false)] {
                for name in names.iter().flatten() {
                    let Some(usage) = usages.get_mut(name) else {
                        continue;
                    };
                    let variable = &usage.declared.variable;
                    let list = if writing {
                        variable.writers.as_ref()
                    } else {
                        variable.readers.as_ref()
                    };
                    if !allowed(list, &usage.declared.project, &project, &hook.name) {
                        usage.violations.push(format!(
                            "`{reference}` isn't allowed to {} `{name}` (allowed: {})",
                            if writing { "write" } else { "read" },
                            list.map(|l| l.join(", ")).unwrap_or_default()
                        ));
                    }
                    if writing {
                        usage.writers.push(reference.clone());
                    } else {
                        usage.readers.push(reference.clone());
                    }
                }
            }
        }
    }
    usages.into_values().collect()
}

/// Variables with their writers, readers and current value
pub fn usage(root: &Path, configs: &[Config]) -> Result<Vec<Usage>, GoatHookerError> {
    let declared = declared(configs)?;
    let values = load(root, &declared);
    let mut usages = usages(configs, &declared);
    for usage in &mut usages {
        usage.value = values.get(&usage.declared.name).cloned();
    }
    Ok(usages)
}

fn store(root: &Path) -> Result<PathBuf, GoatHookerError> {
    Ok(git::data_dir(root)?.join("variables.json"))
}

/// Current values: the stored ones, or the defaults
pub fn load(root: &Path, declared: &BTreeMap<String, Declared>) -> BTreeMap<String, String> {
    let stored: BTreeMap<String, String> = store(root)
        .ok()
        .and_then(|f| fs::read_to_string(f).ok())
        .and_then(|c| serde_json::from_str(&c).ok())
        .unwrap_or_default();
    declared
        .iter()
        .filter_map(|(name, d)| {
            stored
                .get(name)
                .cloned()
                .or_else(|| d.variable.default.clone())
                .map(|v| (name.clone(), v))
        })
        .collect()
}

/// Saves the values (the ones of variables that aren't declared anymore are dropped)
pub fn save(root: &Path, values: &BTreeMap<String, String>) -> Result<(), GoatHookerError> {
    let file = store(root)?;
    fs::create_dir_all(file.parent().unwrap())?;
    fs::write(
        file,
        serde_json::to_string_pretty(values).unwrap_or_default() + "\n",
    )?;
    Ok(())
}

/// Forgets the stored value of a variable (back to its default)
pub fn reset(root: &Path, name: &str) -> Result<(), GoatHookerError> {
    let file = store(root)?;
    let mut stored: BTreeMap<String, String> = fs::read_to_string(&file)
        .ok()
        .and_then(|c| serde_json::from_str(&c).ok())
        .unwrap_or_default();
    if stored.remove(name).is_some() {
        save(root, &stored)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allowed_lists() {
        let list = vec!["bump".to_string(), "api:codegen".to_string()];
        assert!(allowed(None, "root", "web", "x"));
        assert!(allowed(Some(&list), "root", "root", "bump"));
        assert!(!allowed(Some(&list), "root", "web", "bump"));
        assert!(allowed(Some(&list), "root", "api", "codegen"));
        assert!(allowed(Some(&vec!["*".into()]), "root", "web", "x"));
        assert!(allowed(Some(&vec!["web:*".into()]), "root", "web", "x"));
        assert!(allowed(
            Some(&vec!["root:bump".into()]),
            "api",
            "root",
            "bump"
        ));
    }
}
