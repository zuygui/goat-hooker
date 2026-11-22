use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};

use clap::ValueEnum;
use serde::{Deserialize, Serialize};

use crate::{errors::GoatHookerError, git};

pub const HOOK_CONFIG_FILENAME: &str = "hooker.config.json";

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct Config {
    /// Dossiers des sous-projets (globs acceptés, ex: `packages/*`)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub imports: Option<Vec<String>>,
    /// Charge automatiquement tous les `hooker.config.json` du dépôt
    #[serde(skip_serializing_if = "Option::is_none")]
    pub discover: Option<bool>,
    pub hooks: Option<Vec<Hook>>,
    #[serde(skip)]
    pub base_dir: Option<PathBuf>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Hook {
    pub name: String,
    pub trigger: Trigger,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exclude: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cmd: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cmds: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

/// Hooks Git côté client (le nom = le nom du fichier dans `.git/hooks`)
#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Hash, ValueEnum)]
pub enum Trigger {
    #[serde(rename = "pre-commit")]
    #[value(name = "pre-commit")]
    PreCommit,
    #[serde(rename = "prepare-commit-msg")]
    #[value(name = "prepare-commit-msg")]
    PrepareCommitMsg,
    #[serde(rename = "commit-msg")]
    #[value(name = "commit-msg")]
    CommitMsg,
    #[serde(rename = "post-commit")]
    #[value(name = "post-commit")]
    PostCommit,
    #[serde(rename = "pre-rebase")]
    #[value(name = "pre-rebase")]
    PreRebase,
    #[serde(rename = "post-rewrite")]
    #[value(name = "post-rewrite")]
    PostRewrite,
    #[serde(rename = "pre-push")]
    #[value(name = "pre-push")]
    PrePush,
    #[serde(rename = "applypatch-msg")]
    #[value(name = "applypatch-msg")]
    ApplyPatchMsg,
    #[serde(rename = "pre-applypatch")]
    #[value(name = "pre-applypatch")]
    PreApplyPatch,
    #[serde(rename = "post-applypatch")]
    #[value(name = "post-applypatch")]
    PostApplyPatch,
    #[serde(rename = "pre-auto-gc")]
    #[value(name = "pre-auto-gc")]
    PreAutoGc,
    #[serde(rename = "post-checkout")]
    #[value(name = "post-checkout")]
    PostCheckout,
    #[serde(rename = "post-merge")]
    #[value(name = "post-merge")]
    PostMerge,
}

impl Trigger {
    pub const ALL: [Trigger; 13] = [
        Trigger::PreCommit,
        Trigger::PrepareCommitMsg,
        Trigger::CommitMsg,
        Trigger::PostCommit,
        Trigger::PreRebase,
        Trigger::PostRewrite,
        Trigger::PrePush,
        Trigger::ApplyPatchMsg,
        Trigger::PreApplyPatch,
        Trigger::PostApplyPatch,
        Trigger::PreAutoGc,
        Trigger::PostCheckout,
        Trigger::PostMerge,
    ];

    /// Nom du hook côté Git
    pub fn as_str(&self) -> &'static str {
        match self {
            Trigger::PreCommit => "pre-commit",
            Trigger::PrepareCommitMsg => "prepare-commit-msg",
            Trigger::CommitMsg => "commit-msg",
            Trigger::PostCommit => "post-commit",
            Trigger::PreRebase => "pre-rebase",
            Trigger::PostRewrite => "post-rewrite",
            Trigger::PrePush => "pre-push",
            Trigger::ApplyPatchMsg => "applypatch-msg",
            Trigger::PreApplyPatch => "pre-applypatch",
            Trigger::PostApplyPatch => "post-applypatch",
            Trigger::PreAutoGc => "pre-auto-gc",
            Trigger::PostCheckout => "post-checkout",
            Trigger::PostMerge => "post-merge",
        }
    }
}

impl Hook {
    /// Commandes à exécuter, dans l'ordre
    pub fn commands(&self) -> Vec<&str> {
        match (self.cmd.as_ref(), self.cmds.as_ref()) {
            (Some(cmd), _) => vec![cmd.as_str()],
            (None, Some(cmds)) => cmds.iter().map(String::as_str).collect(),
            (None, None) => vec![],
        }
    }

    fn validate(&self) -> Result<(), GoatHookerError> {
        let invalid = |reason: &str| GoatHookerError::InvalidHook {
            name: self.name.clone(),
            reason: reason.to_string(),
        };

        if self.cmd.is_some() && self.cmds.is_some() {
            return Err(invalid("`cmd` and `cmds` can't be used together"));
        }
        if self.cmds.as_ref().is_some_and(|c| c.is_empty()) {
            return Err(invalid("`cmds` is empty"));
        }
        if self.cmd.is_none() && self.cmds.is_none() && self.from.is_none() && self.path.is_none() {
            return Err(invalid(
                "one of `cmd`, `cmds`, `from` or `path` is required",
            ));
        }
        Ok(())
    }
}

impl Config {
    pub fn load_from_file(path: &Path) -> Result<Config, GoatHookerError> {
        let content = fs::read_to_string(path).map_err(GoatHookerError::ReadError)?;
        let mut cfg: Config =
            serde_json::from_str(&content).map_err(|source| GoatHookerError::ParseError {
                path: path.to_path_buf(),
                source,
            })?;

        for hook in cfg.hooks.iter().flatten() {
            hook.validate()?;
        }

        cfg.base_dir = Some(
            path.parent()
                .unwrap_or_else(|| Path::new("."))
                .to_path_buf(),
        );
        Ok(cfg)
    }

    pub fn write_to_dir(&self, dir: &Path) -> Result<PathBuf, GoatHookerError> {
        let path = dir.join(HOOK_CONFIG_FILENAME);
        let content =
            serde_json::to_string_pretty(self).map_err(|source| GoatHookerError::ParseError {
                path: path.clone(),
                source,
            })?;
        fs::write(&path, content + "\n")?;
        Ok(path)
    }
}

/// Loader qui gère la récursivité + détection des cycles
pub struct ConfigLoader {
    visited: HashSet<PathBuf>,
}

impl ConfigLoader {
    pub fn new() -> Self {
        Self {
            visited: HashSet::new(),
        }
    }

    pub fn load_recursive(&mut self, path: &Path) -> Result<Vec<Config>, GoatHookerError> {
        if !path.exists() {
            return Err(GoatHookerError::ConfigNotFound(path.to_path_buf()));
        }
        let abs_path = fs::canonicalize(path)?;

        if !self.visited.insert(abs_path.clone()) {
            // déjà visité → on ignore pour éviter les cycles
            return Ok(vec![]);
        }

        let root = Config::load_from_file(&abs_path)?;
        let mut all_cfg = vec![root.clone()];

        let base_dir = root.base_dir.clone().unwrap();

        for inc in root.imports.iter().flatten() {
            for sub_path in resolve_import(&base_dir, inc)? {
                let sub_cfgs = self.load_recursive(&sub_path)?;
                all_cfg.extend(sub_cfgs);
            }
        }

        // Les configs déjà chargées (dont celle-ci) sont ignorées grâce à `visited`
        if root.discover.unwrap_or(false) {
            for sub_path in git::find_files(&base_dir, HOOK_CONFIG_FILENAME)? {
                let sub_cfgs = self.load_recursive(&sub_path)?;
                all_cfg.extend(sub_cfgs);
            }
        }

        Ok(all_cfg)
    }
}

/// Résout un import en chemins de configs.
/// Un import peut être un dossier (`api`) ou un glob (`packages/*`).
fn resolve_import(base_dir: &Path, inc: &str) -> Result<Vec<PathBuf>, GoatHookerError> {
    let is_glob = inc.contains(['*', '?', '[']);

    if !is_glob {
        let sub_path = base_dir.join(inc).join(HOOK_CONFIG_FILENAME);
        if !sub_path.exists() {
            eprintln!(
                "⚠️ Warning: included path {:?} does not contain {}",
                sub_path, HOOK_CONFIG_FILENAME
            );
            return Ok(vec![]);
        }
        return Ok(vec![sub_path]);
    }

    // Le dossier de base est échappé pour qu'un `[` dans son chemin ne casse pas le glob
    let pattern = format!(
        "{}/{}/{}",
        glob::Pattern::escape(&base_dir.to_string_lossy()),
        inc.trim_start_matches("./").trim_end_matches('/'),
        HOOK_CONFIG_FILENAME
    );
    let paths = glob::glob(&pattern).map_err(|source| GoatHookerError::InvalidImport {
        import: inc.to_string(),
        source,
    })?;

    // Les dossiers sans config sont ignorés silencieusement (ex: `packages/*` avec un dossier vide)
    let mut sub_paths: Vec<PathBuf> = paths.filter_map(Result::ok).collect();
    sub_paths.sort();

    if sub_paths.is_empty() {
        eprintln!(
            "⚠️ Warning: no {} found for import `{}`",
            HOOK_CONFIG_FILENAME, inc
        );
    }
    Ok(sub_paths)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    fn write_json(path: &Path, content: &str) {
        fs::write(path, content).expect("failed to write test json");
    }

    #[test]
    fn load_single_config() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join("hooker.config.json");

        write_json(
            &config_path,
            r#"{ "hooks": [ { "name": "test", "trigger": "pre-commit", "cmd": "true" } ] }"#,
        );

        let mut loader = ConfigLoader::new();
        let cfgs = loader.load_recursive(&config_path).unwrap();

        assert_eq!(cfgs.len(), 1);
        assert_eq!(cfgs[0].hooks.as_ref().unwrap()[0].name, "test");
    }

    #[test]
    fn load_with_imports() {
        let dir = tempdir().unwrap();

        // root config
        let root_path = dir.path().join("hooker.config.json");
        write_json(
            &root_path,
            r#"{ "imports": ["sub"], "hooks": [ { "name": "root-hook", "trigger": "pre-commit", "cmd": "true" } ] }"#,
        );

        // sub config
        let sub_dir = dir.path().join("sub");
        fs::create_dir(&sub_dir).unwrap();
        let sub_config_path = sub_dir.join("hooker.config.json");
        write_json(
            &sub_config_path,
            r#"{ "hooks": [ { "name": "sub-hook", "trigger": "pre-commit", "cmd": "true" } ] }"#,
        );

        let mut loader = ConfigLoader::new();
        let cfgs = loader.load_recursive(&root_path).unwrap();

        let hook_names: Vec<String> = cfgs
            .iter()
            .flat_map(|c| c.hooks.clone().unwrap_or_default())
            .map(|h| h.name)
            .collect();

        assert!(hook_names.contains(&"root-hook".to_string()));
        assert!(hook_names.contains(&"sub-hook".to_string()));
    }

    #[test]
    fn detect_cycles() {
        let dir = tempdir().unwrap();

        // root config imports sub
        let root_path = dir.path().join("hooker.config.json");
        write_json(
            &root_path,
            r#"{ "imports": ["sub"], "hooks": [ { "name": "root", "trigger": "pre-commit", "cmd": "true" } ] }"#,
        );

        // sub config imports root again (cycle)
        let sub_dir = dir.path().join("sub");
        fs::create_dir(&sub_dir).unwrap();
        let sub_path = sub_dir.join("hooker.config.json");
        write_json(
            &sub_path,
            r#"{
                "imports": [".."],
                "hooks": [ { "name": "sub", "trigger": "pre-commit", "cmd": "true" } ]
            }"#,
        );

        let mut loader = ConfigLoader::new();
        let cfgs = loader.load_recursive(&root_path).unwrap();

        // only 2 configs, no infinite recursion
        assert_eq!(cfgs.len(), 2);
    }

    #[test]
    fn missing_config() {
        let dir = tempdir().unwrap();
        let mut loader = ConfigLoader::new();
        let res = loader.load_recursive(&dir.path().join(HOOK_CONFIG_FILENAME));

        assert!(matches!(res, Err(GoatHookerError::ConfigNotFound(_))));
    }

    #[test]
    fn parse_error_keeps_position() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join(HOOK_CONFIG_FILENAME);
        write_json(&config_path, "{ \"hooks\": [ }");

        let mut loader = ConfigLoader::new();
        let err = loader.load_recursive(&config_path).unwrap_err();

        assert!(matches!(err, GoatHookerError::ParseError { .. }));
        assert!(err.to_string().contains("line 1"));
    }

    #[test]
    fn reject_invalid_hooks() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join(HOOK_CONFIG_FILENAME);

        // ni cmd ni cmds
        write_json(
            &config_path,
            r#"{ "hooks": [ { "name": "empty", "trigger": "pre-commit" } ] }"#,
        );
        let err = Config::load_from_file(&config_path).unwrap_err();
        assert!(matches!(err, GoatHookerError::InvalidHook { .. }));

        // cmd et cmds en même temps
        write_json(
            &config_path,
            r#"{ "hooks": [ { "name": "both", "trigger": "pre-commit", "cmd": "a", "cmds": ["b"] } ] }"#,
        );
        let err = Config::load_from_file(&config_path).unwrap_err();
        assert!(matches!(err, GoatHookerError::InvalidHook { .. }));
    }

    #[test]
    fn parse_all_triggers() {
        for trigger in Trigger::ALL {
            let json = format!(r#""{}""#, trigger.as_str());
            let parsed: Trigger = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, trigger);
        }
    }

    fn hook_names(cfgs: &[Config]) -> Vec<String> {
        let mut names: Vec<String> = cfgs
            .iter()
            .flat_map(|c| c.hooks.clone().unwrap_or_default())
            .map(|h| h.name)
            .collect();
        names.sort();
        names
    }

    fn write_sub_config(dir: &Path, hook_name: &str) {
        fs::create_dir_all(dir).unwrap();
        write_json(
            &dir.join(HOOK_CONFIG_FILENAME),
            &format!(
                r#"{{ "hooks": [ {{ "name": "{hook_name}", "trigger": "pre-commit", "cmd": "true" }} ] }}"#
            ),
        );
    }

    #[test]
    fn load_glob_imports() {
        let dir = tempdir().unwrap();
        let root_path = dir.path().join(HOOK_CONFIG_FILENAME);
        write_json(&root_path, r#"{ "imports": ["packages/*"], "hooks": [] }"#);

        write_sub_config(&dir.path().join("packages/a"), "a");
        write_sub_config(&dir.path().join("packages/b"), "b");
        // dossier sans config → ignoré
        fs::create_dir_all(dir.path().join("packages/empty")).unwrap();

        let cfgs = ConfigLoader::new().load_recursive(&root_path).unwrap();
        assert_eq!(hook_names(&cfgs), vec!["a", "b"]);
    }

    #[test]
    fn discover_configs() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        std::process::Command::new("git")
            .args(["init", "-q"])
            .current_dir(root)
            .status()
            .unwrap();
        fs::write(root.join(".gitignore"), "ignored/\n").unwrap();

        write_json(
            &root.join(HOOK_CONFIG_FILENAME),
            r#"{ "discover": true, "imports": ["api"], "hooks": [ { "name": "root", "trigger": "pre-commit", "cmd": "true" } ] }"#,
        );
        write_sub_config(&root.join("api"), "api");
        write_sub_config(&root.join("apps/web"), "web");
        write_sub_config(&root.join("apps/web/nested"), "nested");
        write_sub_config(&root.join("ignored/lib"), "ignored");

        let cfgs = ConfigLoader::new()
            .load_recursive(&root.join(HOOK_CONFIG_FILENAME))
            .unwrap();

        // `api` importé ET découvert → chargé une seule fois ; `ignored/` est ignoré par git
        assert_eq!(hook_names(&cfgs), vec!["api", "nested", "root", "web"]);
    }
}
