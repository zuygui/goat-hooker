//! Data shown by the app: the repository, its projects and their hooks (no UI)

use std::path::{Path, PathBuf};

use goat_hooker::{
    config::{Config, ConfigLoader, HOOK_CONFIG_FILENAME, Hook, SCHEMA_URL, Trigger},
    errors::GoatHookerError,
    git, install,
    matcher::{MatchedHook, Matcher},
    runner::{self, HookInput, HookResult, Output},
};

/// A project of the monorepo (a `hooker.config.json`)
#[derive(Clone)]
pub struct Project {
    pub file: PathBuf,
    /// Folder relative to the root of the repository (`.` for the root)
    pub name: String,
    /// Config as written in the file (without the hooks loaded by `path`)
    pub raw: Config,
}

impl Project {
    pub fn dir(&self) -> PathBuf {
        self.file.parent().unwrap().to_path_buf()
    }

    pub fn hooks(&self) -> &[Hook] {
        self.raw.hooks.as_deref().unwrap_or_default()
    }

    /// Adds (`index = None`) or replaces a hook, then saves the file
    pub fn save_hook(&mut self, index: Option<usize>, hook: Hook) -> Result<(), GoatHookerError> {
        let hooks = self.raw.hooks.get_or_insert_with(Vec::new);
        match index {
            Some(i) if i < hooks.len() => hooks[i] = hook,
            _ => hooks.push(hook),
        }
        self.raw.write_to_file(&self.file)
    }

    pub fn delete_hook(&mut self, index: usize) -> Result<(), GoatHookerError> {
        let hooks = self.raw.hooks.get_or_insert_with(Vec::new);
        if index < hooks.len() {
            hooks.remove(index);
        }
        self.raw.write_to_file(&self.file)
    }
}

/// Hook that would run on the next commit
#[derive(Clone)]
pub struct PreviewItem {
    pub hook: String,
    pub project_dir: PathBuf,
    pub files: Vec<PathBuf>,
}

#[derive(Clone)]
pub struct Repo {
    pub root: PathBuf,
    pub projects: Vec<Project>,
    pub installed: bool,
    pub branch: Option<String>,
    pub staged_files: usize,
}

impl Repo {
    pub fn config_file(&self) -> PathBuf {
        self.root.join(HOOK_CONFIG_FILENAME)
    }

    /// Opens the git repository containing `dir`
    pub fn open(dir: &Path) -> Result<Self, GoatHookerError> {
        let root = git::repo_root(dir)?;
        let mut repo = Self {
            installed: install::is_installed(&root),
            branch: None,
            staged_files: 0,
            root,
            projects: vec![],
        };
        repo.reload()?;
        Ok(repo)
    }

    /// Reloads the configs from the disk
    pub fn reload(&mut self) -> Result<(), GoatHookerError> {
        self.installed = install::is_installed(&self.root);
        self.branch = git::current_branch(&self.root);
        self.staged_files = git::staged_files(&self.root).map(|f| f.len()).unwrap_or(0);
        self.projects.clear();

        let config_file = self.config_file();
        if !config_file.exists() {
            return Ok(());
        }

        for config in ConfigLoader::new().load_recursive(&config_file)? {
            let file = config.file.clone().unwrap();
            let dir = file.parent().unwrap();
            let name = project_name(&self.root.canonicalize()?, dir);
            self.projects.push(Project {
                raw: Config::load_from_file(&file)?,
                file,
                name,
            });
        }
        Ok(())
    }

    /// Creates `hooker.config.json` at the root (like `hooker init`)
    pub fn init_config(&mut self) -> Result<(), GoatHookerError> {
        if !self.config_file().exists() {
            Config {
                schema: Some(SCHEMA_URL.to_string()),
                hooks: Some(vec![]),
                ..Default::default()
            }
            .write_to_dir(&self.root)?;
        }
        self.reload()
    }

    pub fn install(&mut self) -> Result<Vec<PathBuf>, GoatHookerError> {
        let skipped = install::install_hooks(&self.root, false)?;
        self.installed = install::is_installed(&self.root);
        Ok(skipped)
    }

    pub(crate) fn load_configs(&self) -> Result<Vec<Config>, GoatHookerError> {
        if !self.config_file().exists() {
            return Ok(vec![]);
        }
        ConfigLoader::new().load_recursive(&self.config_file())
    }

    /// Pre-commit hooks that would run on the staged files
    pub fn preview_pre_commit(&self) -> Result<Vec<PreviewItem>, GoatHookerError> {
        let configs = self.load_configs()?;
        let matcher = Matcher::new(&configs)?;
        let staged = git::staged_files(&self.root)?;
        Ok(matcher
            .hooks_to_run(Trigger::PreCommit, Some(&staged))
            .into_iter()
            .map(|matched| PreviewItem {
                hook: matched.hook.name.clone(),
                project_dir: matched.base_dir,
                files: matched.files,
            })
            .collect())
    }

    /// Runs all the hooks of a trigger, like Git would
    pub fn run_trigger(&self, trigger: Trigger) -> Result<Vec<HookResult>, GoatHookerError> {
        let configs = self.load_configs()?;
        runner::run_trigger(
            &self.root,
            &configs,
            trigger,
            &HookInput::default(),
            Output::Capture,
            None,
        )
    }

    /// Runs a single hook of a project.
    /// For a pre-commit, on the staged files it targets.
    pub fn run_hook(
        &self,
        project: &Project,
        hook_name: &str,
    ) -> Result<HookResult, GoatHookerError> {
        let configs = self.load_configs()?;
        let project_dir = project.dir().canonicalize()?;

        let Some(hook) = configs
            .iter()
            .filter(|c| c.base_dir.as_deref() == Some(project_dir.as_path()))
            .flat_map(|c| c.hooks.iter().flatten())
            .find(|h| h.name == hook_name)
        else {
            return Err(GoatHookerError::InvalidHook {
                name: hook_name.to_string(),
                reason: "hook not found".to_string(),
            });
        };

        let files = if hook.trigger == Some(Trigger::PreCommit) {
            let staged = git::staged_files(&self.root)?;
            Matcher::new(&configs)?
                .hooks_to_run(Trigger::PreCommit, Some(&staged))
                .into_iter()
                .find(|m| m.hook.name == hook_name && m.base_dir == project_dir)
                .map(|m| m.files)
                .unwrap_or_default()
        } else {
            vec![]
        };

        let matched = MatchedHook {
            hook,
            base_dir: project_dir,
            files,
            affected_by: None,
        };
        goat_hooker::trust::check(&self.root, [hook])?;
        let shared = runner::Shared::load(&self.root, &configs)?;
        let result = runner::run_hook(&matched, &HookInput::default(), Output::Capture, &shared)?;
        if result.success {
            shared.save(&self.root)?;
        }
        Ok(result)
    }
}

fn project_name(root: &Path, dir: &Path) -> String {
    match dir.strip_prefix(root) {
        Ok(rel) if rel.as_os_str().is_empty() => ".".to_string(),
        Ok(rel) => rel.display().to_string(),
        Err(_) => dir.display().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, process::Command};
    use tempfile::tempdir;

    fn git(dir: &Path, args: &[&str]) {
        let status = Command::new("git")
            .args(args)
            .current_dir(dir)
            .status()
            .unwrap();
        assert!(status.success());
    }

    fn setup() -> (tempfile::TempDir, PathBuf) {
        let dir = tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        git(&root, &["init", "-q"]);
        fs::write(
            root.join(HOOK_CONFIG_FILENAME),
            r#"{ "imports": ["api"], "hooks": [ { "name": "root", "trigger": "pre-commit", "include": ["*.md"], "cmd": "echo root {files}" } ] }"#,
        )
        .unwrap();
        fs::create_dir_all(root.join("api")).unwrap();
        fs::write(
            root.join("api").join(HOOK_CONFIG_FILENAME),
            r#"{ "hooks": [ { "name": "api", "trigger": "pre-commit", "include": ["*.rs"], "cmd": "echo api {files}" } ] }"#,
        )
        .unwrap();
        (dir, root)
    }

    #[test]
    fn open_repo_lists_projects() {
        let (_dir, root) = setup();
        let repo = Repo::open(&root.join("api")).unwrap();

        let names: Vec<&str> = repo.projects.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, vec![".", "api"]);
        assert!(!repo.installed);
    }

    #[test]
    fn edit_hooks() {
        let (_dir, root) = setup();
        let mut repo = Repo::open(&root).unwrap();

        let hook = Hook {
            name: "fmt".to_string(),
            trigger: Some(Trigger::PreCommit),
            cmd: Some("cargo fmt".to_string()),
            ..Default::default()
        };
        repo.projects[1].save_hook(None, hook).unwrap();
        repo.projects[1].delete_hook(0).unwrap();
        repo.reload().unwrap();

        let names: Vec<&str> = repo.projects[1]
            .hooks()
            .iter()
            .map(|h| h.name.as_str())
            .collect();
        assert_eq!(names, vec!["fmt"]);
    }

    #[test]
    fn preview_and_run() {
        let (_dir, root) = setup();
        fs::write(root.join("api/lib.rs"), "").unwrap();
        git(&root, &["add", "api/lib.rs"]);
        let repo = Repo::open(&root).unwrap();

        let preview = repo.preview_pre_commit().unwrap();
        assert_eq!(preview.len(), 1);
        assert_eq!(preview[0].hook, "api");
        assert_eq!(preview[0].project_dir, root.join("api"));
        assert_eq!(preview[0].files, vec![PathBuf::from("lib.rs")]);

        let result = repo.run_hook(&repo.projects[1], "api").unwrap();
        assert_eq!(result.commands[0].output, "api lib.rs\n");

        let results = repo.run_trigger(Trigger::PreCommit).unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].success);
    }

    #[test]
    fn init_and_install() {
        let dir = tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        git(&root, &["init", "-q"]);

        let mut repo = Repo::open(&root).unwrap();
        assert!(repo.projects.is_empty());
        repo.init_config().unwrap();
        assert_eq!(repo.projects.len(), 1);

        repo.install().unwrap();
        assert!(repo.installed);
    }
}

/// Hooks of another repository waiting to be trusted
#[derive(Clone)]
pub struct Untrusted {
    pub origin: goat_hooker::config::Origin,
    pub hooks: Vec<Hook>,
    /// What changed since the trusted version (`None`: never trusted)
    pub changes: Option<Vec<String>>,
}

impl Repo {
    /// Variables with their writers, readers and value
    pub fn variables(&self) -> Result<Vec<goat_hooker::variables::Usage>, GoatHookerError> {
        let configs = self.load_configs()?;
        goat_hooker::variables::usage(&self.root, &configs)
    }

    pub fn reset_variable(&self, name: &str) -> Result<(), GoatHookerError> {
        goat_hooker::variables::reset(&self.root, name)
    }

    /// Hooks files of other repositories that must be reviewed before they run
    pub fn untrusted(&self) -> Vec<Untrusted> {
        let Ok(configs) = self.load_configs() else {
            return vec![];
        };
        let hooks: Vec<&Hook> = configs
            .iter()
            .flat_map(|c| c.hooks.iter().flatten())
            .collect();
        goat_hooker::trust::untrusted(&self.root, hooks.iter().copied())
            .into_iter()
            .map(|origin| Untrusted {
                changes: goat_hooker::trust::changes(&self.root, &origin),
                hooks: hooks
                    .iter()
                    .filter(|h| h.origin.as_ref() == Some(&origin))
                    .map(|h| (*h).clone())
                    .collect(),
                origin,
            })
            .collect()
    }

    pub fn trust(&self, origin: &goat_hooker::config::Origin) -> Result<(), GoatHookerError> {
        goat_hooker::trust::trust(&self.root, std::slice::from_ref(origin))
    }

    /// Adds a hooks file found on GitHub to a project
    pub fn add_shared(
        &mut self,
        project: usize,
        name: &str,
        repo: &goat_hooker::hub::SharedRepo,
        file: &goat_hooker::hub::SharedFile,
        with: std::collections::BTreeMap<String, serde_json::Value>,
    ) -> Result<(), GoatHookerError> {
        let config_file = self.projects[project].file.clone();
        goat_hooker::hub::add(&self.root, &config_file, name, repo, file, with)?;
        self.reload()
    }

    /// Configs of husky, lint-staged, lefthook or pre-commit found in the repository
    pub fn import_preview(&self) -> Option<goat_hooker::import::Imported> {
        goat_hooker::import::detect(&self.root)
            .ok()
            .filter(|i| !i.is_empty())
    }

    pub fn import(&mut self) -> Result<goat_hooker::import::Imported, GoatHookerError> {
        let imported = goat_hooker::import::detect(&self.root)?;
        goat_hooker::import::write(&self.root, &imported, true)?;
        self.reload()?;
        Ok(imported)
    }
}

impl Repo {
    /// Fetches the last version of a repository of hooks (to review before it runs)
    pub fn update_remote(&mut self, url: &str, rev: Option<&str>) -> Result<(), GoatHookerError> {
        let repo = goat_hooker::remote::fetch(url, rev)?;
        goat_hooker::remote::update(&repo, rev)?;
        self.reload()
    }

    /// Pins a hook loading another repository to the commit in use
    pub fn pin_hook(&mut self, project: usize, index: usize) -> Result<(), GoatHookerError> {
        let mut hook = self.projects[project].hooks()[index].clone();
        let Some(url) = hook.from.clone() else {
            return Ok(());
        };
        let repo = goat_hooker::remote::fetch(&url, hook.rev.as_deref())?;
        if let Some(commit) = goat_hooker::remote::head(&repo) {
            hook.rev = Some(commit);
            self.projects[project].save_hook(Some(index), hook)?;
        }
        self.reload()
    }
}

impl Repo {
    /// Names the JSON editor of a project suggests: hooks, variables, outputs
    pub fn known(&self, project: usize) -> crate::json_complete::Known {
        let mut known = crate::json_complete::Known::default();
        let current = self.projects.get(project).map(|p| p.name.clone());
        for p in &self.projects {
            let prefix = if Some(&p.name) == current.as_ref() {
                String::new()
            } else if p.name == "." {
                "root:".to_string()
            } else {
                format!("{}:", p.name)
            };
            for hook in p.hooks() {
                known.hooks.push(format!("{prefix}{}", hook.name));
                for output in hook.outputs.iter().flatten() {
                    known
                        .outputs
                        .push(format!("{prefix}{}.{output}", hook.name));
                }
                for step in hook.steps.iter().flatten() {
                    known
                        .hooks
                        .push(format!("{prefix}{}/{}", hook.name, step.id));
                }
            }
            known.variables.extend(
                p.raw
                    .variables
                    .iter()
                    .flatten()
                    .map(|(name, _)| name.clone()),
            );
        }
        known
    }
}
