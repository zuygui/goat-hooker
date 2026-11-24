//! Dependencies between the projects of a mono-repo, to run the hooks of the projects
//! affected by a change (`"affected": true`).
//!
//! Found in the manifests of the projects:
//! - Cargo: `path` dependencies (`lib = { path = "../lib" }`)
//! - npm / pnpm / yarn / bun: dependencies on another package of the repository (by name)
//! - Go: `replace ... => ../lib` in `go.mod`
//! - Python: `path = "../lib"` dependencies in `pyproject.toml` (uv, poetry)
//!
//! and in `depends_on` of the configs (folders of other projects).

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

use crate::config::Config;

/// Projects (folders) each project depends on, directly
#[derive(Debug, Default, Clone)]
pub struct Dependencies {
    direct: BTreeMap<PathBuf, BTreeSet<PathBuf>>,
}

fn normalize(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

/// The project containing a path (the deepest one)
fn owner<'a>(projects: &'a [PathBuf], path: &Path) -> Option<&'a PathBuf> {
    projects
        .iter()
        .filter(|p| path.starts_with(p))
        .max_by_key(|p| p.components().count())
}

/// `path = "..."` values of a manifest (Cargo.toml, pyproject.toml)
fn path_values(content: &str) -> Vec<String> {
    let mut paths = Vec::new();
    for line in content.lines() {
        let line = line.split('#').next().unwrap_or_default();
        let mut rest = line;
        while let Some(i) = rest.find("path") {
            let after = rest[i + 4..].trim_start();
            if let Some(value) = after.strip_prefix('=') {
                let value = value.trim_start();
                if let Some(value) = value.strip_prefix('"')
                    && let Some(end) = value.find('"')
                {
                    paths.push(value[..end].to_string());
                }
            }
            rest = &rest[i + 4..];
        }
    }
    paths
}

/// Names of the packages a package.json depends on
fn npm_dependencies(package: &serde_json::Value) -> Vec<String> {
    [
        "dependencies",
        "devDependencies",
        "peerDependencies",
        "optionalDependencies",
    ]
    .iter()
    .filter_map(|k| package.get(k).and_then(|d| d.as_object()))
    .flat_map(|d| d.keys().cloned())
    .collect()
}

impl Dependencies {
    /// Reads the manifests of the projects
    pub fn detect(configs: &[Config]) -> Self {
        let projects: Vec<PathBuf> = configs
            .iter()
            .filter_map(|c| c.base_dir.as_deref())
            .map(normalize)
            .collect();

        // package.json names of the projects
        let mut npm_names: BTreeMap<String, PathBuf> = BTreeMap::new();
        for project in &projects {
            if let Some(name) = fs::read_to_string(project.join("package.json"))
                .ok()
                .and_then(|c| serde_json::from_str::<serde_json::Value>(&c).ok())
                .and_then(|p| p["name"].as_str().map(str::to_string))
            {
                npm_names.insert(name, project.clone());
            }
        }

        let mut direct: BTreeMap<PathBuf, BTreeSet<PathBuf>> = BTreeMap::new();
        for (config, project) in configs
            .iter()
            .filter(|c| c.base_dir.is_some())
            .zip(&projects)
        {
            let mut deps: BTreeSet<PathBuf> = BTreeSet::new();
            let mut add_path = |relative: &str| {
                let target = normalize(&project.join(relative));
                if let Some(owner) = owner(&projects, &target)
                    && owner != project
                {
                    deps.insert(owner.clone());
                }
            };

            for depends_on in config.depends_on.iter().flatten() {
                add_path(depends_on);
            }
            for manifest in ["Cargo.toml", "pyproject.toml"] {
                if let Ok(content) = fs::read_to_string(project.join(manifest)) {
                    for path in path_values(&content) {
                        add_path(&path);
                    }
                }
            }
            if let Ok(content) = fs::read_to_string(project.join("go.mod")) {
                for line in content.lines() {
                    if let Some((_, target)) = line.split_once("=>") {
                        let target = target.split_whitespace().next().unwrap_or_default();
                        if target.starts_with('.') {
                            add_path(target);
                        }
                    }
                }
            }
            if let Some(package) = fs::read_to_string(project.join("package.json"))
                .ok()
                .and_then(|c| serde_json::from_str::<serde_json::Value>(&c).ok())
            {
                for name in npm_dependencies(&package) {
                    if let Some(target) = npm_names.get(&name)
                        && target != project
                    {
                        deps.insert(target.clone());
                    }
                }
            }
            direct.insert(project.clone(), deps);
        }
        Self { direct }
    }

    /// Projects a project depends on directly
    pub fn of(&self, project: &Path) -> Vec<PathBuf> {
        self.direct
            .get(&normalize(project))
            .map(|d| d.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// The project among `changed` that `project` depends on (directly or not), if any
    pub fn affected_by(&self, project: &Path, changed: &BTreeSet<PathBuf>) -> Option<PathBuf> {
        let mut seen = BTreeSet::new();
        let mut stack = vec![normalize(project)];
        while let Some(current) = stack.pop() {
            for dep in self.direct.get(&current).into_iter().flatten() {
                if changed.contains(dep) {
                    return Some(dep.clone());
                }
                if seen.insert(dep.clone()) {
                    stack.push(dep.clone());
                }
            }
        }
        None
    }

    /// Every project, with the projects it depends on directly
    pub fn all(&self) -> Vec<(PathBuf, Vec<PathBuf>)> {
        self.direct
            .iter()
            .map(|(project, deps)| (project.clone(), deps.iter().cloned().collect()))
            .collect()
    }

    /// Projects depending on `project`, directly or not
    pub fn dependents(&self, project: &Path) -> Vec<PathBuf> {
        let changed: BTreeSet<PathBuf> = [normalize(project)].into();
        self.direct
            .keys()
            .filter(|p| **p != normalize(project) && self.affected_by(p, &changed).is_some())
            .cloned()
            .collect()
    }

    /// Projects containing the files (the deepest project of each file)
    pub fn changed_projects(&self, files: &[PathBuf]) -> BTreeSet<PathBuf> {
        let projects: Vec<PathBuf> = self.direct.keys().cloned().collect();
        crate::matcher::normalize_files(files)
            .iter()
            .filter_map(|f| owner(&projects, f).cloned())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(dir: &Path) -> Config {
        Config {
            base_dir: Some(dir.to_path_buf()),
            ..Default::default()
        }
    }

    #[test]
    fn manifests() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        for project in [
            "core", "api", "web", "ui", "go/app", "go/lib", "py/app", "py/lib",
        ] {
            fs::create_dir_all(root.join(project)).unwrap();
        }
        fs::write(
            root.join("api/Cargo.toml"),
            "[dependencies]\ncore = { path = \"../core\" } # local\nserde = \"1\"\n",
        )
        .unwrap();
        fs::write(root.join("ui/package.json"), r#"{ "name": "@acme/ui" }"#).unwrap();
        fs::write(
            root.join("web/package.json"),
            r#"{ "name": "web", "dependencies": { "@acme/ui": "workspace:*", "react": "18" } }"#,
        )
        .unwrap();
        fs::write(
            root.join("go/app/go.mod"),
            "module app\nreplace example.com/lib => ../lib\n",
        )
        .unwrap();
        fs::write(
            root.join("py/app/pyproject.toml"),
            "[tool.uv.sources]\nlib = { path = \"../lib\", editable = true }\n",
        )
        .unwrap();

        let configs: Vec<Config> = [
            "", "core", "api", "web", "ui", "go/app", "go/lib", "py/app", "py/lib",
        ]
        .iter()
        .map(|p| config(&root.join(p)))
        .collect();
        let deps = Dependencies::detect(&configs);
        assert_eq!(deps.of(&root.join("api")), vec![root.join("core")]);
        assert_eq!(deps.of(&root.join("web")), vec![root.join("ui")]);
        assert_eq!(deps.of(&root.join("go/app")), vec![root.join("go/lib")]);
        assert_eq!(deps.of(&root.join("py/app")), vec![root.join("py/lib")]);

        let changed = deps.changed_projects(&[root.join("core/src/lib.rs")]);
        assert_eq!(changed, BTreeSet::from([root.join("core")]));
        assert_eq!(
            deps.affected_by(&root.join("api"), &changed),
            Some(root.join("core"))
        );
        assert_eq!(deps.affected_by(&root.join("web"), &changed), None);
    }
}
