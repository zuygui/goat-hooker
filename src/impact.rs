//! What a file triggers: the hooks of each trigger that target it, and the projects that
//! depend on its project (their `affected` hooks run too). Used by the Impact page of the app.

use std::path::{Path, PathBuf};

use crate::{
    config::{Config, Trigger},
    deps::Dependencies,
    errors::GoatHookerError,
    matcher::Matcher,
};

#[derive(Debug, Clone)]
pub struct ImpactedHook {
    pub trigger: Trigger,
    pub name: String,
    /// Folder of its project
    pub project: PathBuf,
    /// Runs because a project it depends on changed (`affected`), not for the file itself
    pub affected_by: Option<PathBuf>,
}

#[derive(Debug, Clone, Default)]
pub struct Impact {
    /// Project containing the file
    pub project: Option<PathBuf>,
    /// Projects depending on it, directly or not
    pub dependents: Vec<PathBuf>,
    pub hooks: Vec<ImpactedHook>,
}

/// Impact of a change of `file` (absolute path)
pub fn of(configs: &[Config], file: &Path) -> Result<Impact, GoatHookerError> {
    let matcher = Matcher::new(configs)?;
    let dependencies = Dependencies::detect(configs);
    let files = [file.to_path_buf()];
    let project = dependencies.changed_projects(&files).into_iter().next();
    let dependents = project
        .as_deref()
        .map(|p| dependencies.dependents(p))
        .unwrap_or_default();

    let mut hooks = Vec::new();
    for trigger in [Trigger::PreCommit, Trigger::PrePush] {
        for matched in matcher.hooks_to_run(trigger, Some(&files)) {
            hooks.push(ImpactedHook {
                trigger,
                name: matched.hook.name.clone(),
                project: matched.base_dir.clone(),
                affected_by: matched.affected_by.clone(),
            });
        }
    }
    Ok(Impact {
        project,
        dependents,
        hooks,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_triggers_its_hooks_and_the_affected_ones() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        std::fs::create_dir_all(root.join("core/src")).unwrap();
        std::fs::create_dir_all(root.join("api")).unwrap();
        std::fs::write(
            root.join("hooker.config.json"),
            r#"{ "imports": ["core", "api"] }"#,
        )
        .unwrap();
        std::fs::write(
            root.join("core/hooker.config.json"),
            r#"{ "hooks": [ { "name": "fmt", "trigger": "pre-commit", "cmd": "x", "include": ["**/*.rs"] },
                            { "name": "docs", "trigger": "pre-commit", "cmd": "x", "include": ["*.md"] } ] }"#,
        )
        .unwrap();
        std::fs::write(
            root.join("api/hooker.config.json"),
            r#"{ "depends_on": ["../core"],
                 "hooks": [ { "name": "test", "trigger": "pre-push", "cmd": "x", "include": ["**/*.rs"], "affected": true } ] }"#,
        )
        .unwrap();
        let configs = crate::config::ConfigLoader::new()
            .load_recursive(&root.join("hooker.config.json"))
            .unwrap();
        let impact = of(&configs, &root.join("core/src/lib.rs")).unwrap();
        assert_eq!(impact.project, Some(root.join("core")));
        assert_eq!(impact.dependents, [root.join("api")]);
        let names: Vec<(&str, bool)> = impact
            .hooks
            .iter()
            .map(|h| (h.name.as_str(), h.affected_by.is_some()))
            .collect();
        assert_eq!(names, [("fmt", false), ("test", true)]);
    }
}
