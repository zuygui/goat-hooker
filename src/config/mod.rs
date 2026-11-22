use std::{
    collections::{BTreeMap, HashSet},
    fs,
    path::{Path, PathBuf},
};

use clap::ValueEnum;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{errors::GoatHookerError, git, remote};

pub const HOOK_CONFIG_FILENAME: &str = "hooker.config.json";

/// URL du schéma, ajoutée par `hooker init` (autocomplétion dans les éditeurs)
pub const SCHEMA_URL: &str =
    "https://raw.githubusercontent.com/zuygui/goat-hooker/main/schema.json";

#[derive(Debug, Default, Serialize, Deserialize, Clone, JsonSchema)]
#[schemars(title = "Goat Hooker config")]
pub struct Config {
    #[serde(rename = "$schema", skip_serializing_if = "Option::is_none")]
    #[schemars(description = "JSON schema of the file")]
    pub schema: Option<String>,

    /// Dossiers des sous-projets (globs acceptés, ex: `packages/*`)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(
        description = "Sub-projects folders, each one with its own hooker.config.json. Globs are supported (packages/*)"
    )]
    pub imports: Option<Vec<String>>,

    /// Charge automatiquement tous les `hooker.config.json` du dépôt
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(
        description = "Load every hooker.config.json of the repository (files ignored by git are skipped)"
    )]
    pub discover: Option<bool>,

    /// Lance les hooks en parallèle (lu sur la config racine)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(description = "Run hooks in parallel (root config only)")]
    pub parallel: Option<bool>,

    #[schemars(description = "Hooks of this project")]
    pub hooks: Option<Vec<Hook>>,

    #[serde(skip)]
    pub base_dir: Option<PathBuf>,

    /// Fichier d'où vient la config
    #[serde(skip)]
    pub file: Option<PathBuf>,
}

#[derive(Debug, Default, Serialize, Deserialize, Clone, JsonSchema)]
pub struct Hook {
    #[schemars(description = "Hook name")]
    pub name: String,

    /// Optionnel seulement pour les hooks qui chargent un fichier (`path`)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(description = "Git hook that runs this hook (not used with `path`)")]
    pub trigger: Option<Trigger>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(
        description = "Glob patterns, relative to the config folder. Without it, the hook targets all files"
    )]
    pub include: Option<Vec<String>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(description = "Glob patterns of files to ignore")]
    pub exclude: Option<Vec<String>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(
        description = "Shell command, run from the config folder. {files} is replaced by the targeted files"
    )]
    pub cmd: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(description = "Shell commands, run in order")]
    pub cmds: Option<Vec<String>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(description = "Re-stage the targeted files after the hook (for formatters)")]
    pub stage_fixed: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(description = "Git repository URL to load the hooks file from (requires `path`)")]
    pub from: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(description = "Branch or tag of `from` (default branch if missing)")]
    pub rev: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(
        description = "Hooks file to load, relative to the config folder (or to the root of `from`)"
    )]
    pub path: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(
        description = "Hooks this one waits for: `name`, `name/step` or `project:name` (same trigger)"
    )]
    pub needs: Option<Vec<String>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(
        description = "Values from other hooks, available as `{name}` and `$name` in commands: `{ \"name\": \"hook.output\" }` (outputs: files, stdout, status or a declared output)"
    )]
    pub inputs: Option<BTreeMap<String, String>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(
        description = "Outputs this hook writes with `echo \"name=value\" >> $HOOKER_OUTPUT`"
    )]
    pub outputs: Option<Vec<String>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(description = "Steps of this hook, each one can wait for the others")]
    pub steps: Option<Vec<Step>>,
}

// Une étape d'un hook (un nœud dans le graphe)
#[derive(Debug, Default, Serialize, Deserialize, Clone, JsonSchema)]
#[schemars(description = "A step of a hook: a node of the graph")]
pub struct Step {
    #[schemars(description = "Step id, unique in the hook")]
    pub id: String,

    #[schemars(description = "Shell command of the step")]
    pub cmd: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(description = "Steps (or hooks) this one waits for")]
    pub needs: Option<Vec<String>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(description = "Values from other steps or hooks: `{ \"name\": \"step.output\" }`")]
    pub inputs: Option<BTreeMap<String, String>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(description = "Outputs this step writes to $HOOKER_OUTPUT")]
    pub outputs: Option<Vec<String>>,
}

/// Sorties de chaque nœud, sans avoir à les déclarer
pub const BUILTIN_OUTPUTS: [&str; 3] = ["files", "stdout", "status"];

/// Nom utilisable comme variable shell (`$name`)
fn is_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

// Hooks Git côté client (le nom = le nom du fichier dans `.git/hooks`)
#[derive(
    Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Hash, ValueEnum, JsonSchema,
)]
#[schemars(description = "Client-side Git hook")]
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
    /// (les étapes, dans l'ordre du fichier, pour un hook avec `steps`)
    pub fn commands(&self) -> Vec<&str> {
        match (self.cmd.as_ref(), self.cmds.as_ref(), self.steps.as_ref()) {
            (Some(cmd), _, _) => vec![cmd.as_str()],
            (None, Some(cmds), _) => cmds.iter().map(String::as_str).collect(),
            (None, None, Some(steps)) => steps.iter().map(|s| s.cmd.as_str()).collect(),
            (None, None, None) => vec![],
        }
    }

    /// Validation des champs du graphe (`needs`, `inputs`, `outputs`)
    fn validate_links(
        name: &str,
        inputs: Option<&BTreeMap<String, String>>,
        outputs: Option<&Vec<String>>,
    ) -> Result<(), String> {
        for (input, source) in inputs.into_iter().flatten() {
            if !is_identifier(input) {
                return Err(format!(
                    "input `{input}` of `{name}` must be a valid variable name (letters, digits, _)"
                ));
            }
            if !source.contains('.') {
                return Err(format!(
                    "input `{input}` of `{name}` must look like `hook.output` (got `{source}`)"
                ));
            }
        }
        for output in outputs.into_iter().flatten() {
            if !is_identifier(output) {
                return Err(format!(
                    "output `{output}` of `{name}` must be a valid variable name"
                ));
            }
            if BUILTIN_OUTPUTS.contains(&output.as_str()) {
                return Err(format!(
                    "output `{output}` of `{name}` is built in, no need to declare it"
                ));
            }
        }
        Ok(())
    }

    /// Hook qui charge les hooks d'un fichier (`path`, éventuellement depuis `from`)
    pub fn is_import(&self) -> bool {
        self.path.is_some()
    }

    pub fn validate(&self) -> Result<(), GoatHookerError> {
        let invalid = |reason: &str| GoatHookerError::InvalidHook {
            name: self.name.clone(),
            reason: reason.to_string(),
        };

        if self.from.is_some() && self.path.is_none() {
            return Err(invalid("`from` requires `path`"));
        }
        if self.rev.is_some() && self.from.is_none() {
            return Err(invalid("`rev` requires `from`"));
        }
        if self.is_import() {
            if self.cmd.is_some() || self.cmds.is_some() {
                return Err(invalid("`path` can't be used with `cmd` or `cmds`"));
            }
            return Ok(());
        }

        if self.trigger.is_none() {
            return Err(invalid("`trigger` is required"));
        }
        Self::validate_links(&self.name, self.inputs.as_ref(), self.outputs.as_ref())
            .map_err(|reason| invalid(&reason))?;

        if let Some(steps) = self.steps.as_ref() {
            if self.cmd.is_some() || self.cmds.is_some() {
                return Err(invalid("`steps` can't be used with `cmd` or `cmds`"));
            }
            if steps.is_empty() {
                return Err(invalid("`steps` is empty"));
            }
            let mut ids = HashSet::new();
            for step in steps {
                if step.id.is_empty() || step.id.contains(['/', '.', ':']) {
                    return Err(invalid(&format!(
                        "step id `{}` can't be empty or contain `/`, `.` or `:`",
                        step.id
                    )));
                }
                if !ids.insert(step.id.as_str()) {
                    return Err(invalid(&format!("two steps are named `{}`", step.id)));
                }
                Self::validate_links(&step.id, step.inputs.as_ref(), step.outputs.as_ref())
                    .map_err(|reason| invalid(&reason))?;
            }
            return Ok(());
        }

        if self.cmd.is_some() && self.cmds.is_some() {
            return Err(invalid("`cmd` and `cmds` can't be used together"));
        }
        if self.cmds.as_ref().is_some_and(|c| c.is_empty()) {
            return Err(invalid("`cmds` is empty"));
        }
        if self.cmd.is_none() && self.cmds.is_none() {
            return Err(invalid(
                "one of `cmd`, `cmds`, `steps` or `path` is required",
            ));
        }
        Ok(())
    }
}

/// JSON Schema de `hooker.config.json`
pub fn json_schema() -> String {
    let schema = schemars::schema_for!(Config);
    serde_json::to_string_pretty(&schema).unwrap() + "\n"
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
        cfg.file = Some(path.to_path_buf());
        Ok(cfg)
    }

    pub fn write_to_dir(&self, dir: &Path) -> Result<PathBuf, GoatHookerError> {
        let path = dir.join(HOOK_CONFIG_FILENAME);
        self.write_to_file(&path)?;
        Ok(path)
    }

    pub fn write_to_file(&self, path: &Path) -> Result<(), GoatHookerError> {
        let content =
            serde_json::to_string_pretty(self).map_err(|source| GoatHookerError::ParseError {
                path: path.to_path_buf(),
                source,
            })?;
        fs::write(path, content + "\n")?;
        Ok(())
    }
}

/// Nombre max de fichiers de hooks imbriqués (`path` qui charge un `path`...)
const MAX_HOOKS_FILES_DEPTH: usize = 10;

/// Loader qui gère la récursivité + détection des cycles
#[derive(Default)]
pub struct ConfigLoader {
    visited: HashSet<PathBuf>,
    /// Met à jour les repos `from` déjà en cache (`hooker update`)
    refresh_remotes: bool,
    refreshed: HashSet<PathBuf>,
}

impl ConfigLoader {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn refresh_remotes(mut self) -> Self {
        self.refresh_remotes = true;
        self
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

        let mut root = Config::load_from_file(&abs_path)?;
        let base_dir = root.base_dir.clone().unwrap();

        // Remplace les hooks `path` par les hooks du fichier chargé
        if let Some(hooks) = root.hooks.take() {
            root.hooks = Some(self.expand_hooks(hooks, &base_dir, 0)?);
        }

        let mut all_cfg = vec![root.clone()];

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

    /// Charge les hooks des fichiers `path` (locaux ou depuis un repo `from`).
    /// Les hooks chargés tournent depuis le projet qui les importe.
    fn expand_hooks(
        &mut self,
        hooks: Vec<Hook>,
        base_dir: &Path,
        depth: usize,
    ) -> Result<Vec<Hook>, GoatHookerError> {
        let mut expanded = Vec::new();

        for hook in hooks {
            let Some(path) = hook.path.as_ref() else {
                expanded.push(hook);
                continue;
            };

            if depth >= MAX_HOOKS_FILES_DEPTH {
                return Err(GoatHookerError::InvalidHook {
                    name: hook.name.clone(),
                    reason: format!("more than {MAX_HOOKS_FILES_DEPTH} nested hooks files"),
                });
            }

            let file = match hook.from.as_ref() {
                Some(url) => {
                    let repo = remote::fetch(url, hook.rev.as_deref())?;
                    if self.refresh_remotes && self.refreshed.insert(repo.clone()) {
                        remote::update(&repo, hook.rev.as_deref())?;
                    }
                    repo.join(path)
                }
                None => base_dir.join(path),
            };
            if !file.exists() {
                return Err(GoatHookerError::ConfigNotFound(file));
            }

            let shared = Config::load_from_file(&file)?;
            let shared_dir = shared.base_dir.clone().unwrap();
            let shared_hooks = shared.hooks.unwrap_or_default();

            // Un fichier de hooks peut lui-même charger des fichiers (relatifs à lui)
            expanded.extend(self.expand_hooks(shared_hooks, &shared_dir, depth + 1)?);
        }

        Ok(expanded)
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

    #[test]
    fn load_hooks_from_local_file() {
        let dir = tempdir().unwrap();
        let root_path = dir.path().join(HOOK_CONFIG_FILENAME);
        write_json(
            &root_path,
            r#"{ "hooks": [
                { "name": "own", "trigger": "pre-commit", "cmd": "true" },
                { "name": "shared", "path": "hooks/shared.json" }
            ] }"#,
        );
        fs::create_dir_all(dir.path().join("hooks")).unwrap();
        write_json(
            &dir.path().join("hooks/shared.json"),
            r#"{ "hooks": [
                { "name": "lint", "trigger": "pre-commit", "cmd": "true" },
                { "name": "nested", "path": "nested.json" }
            ] }"#,
        );
        // relatif au fichier qui l'importe
        write_json(
            &dir.path().join("hooks/nested.json"),
            r#"{ "hooks": [ { "name": "fmt", "trigger": "pre-commit", "cmd": "true" } ] }"#,
        );

        let cfgs = ConfigLoader::new().load_recursive(&root_path).unwrap();
        assert_eq!(hook_names(&cfgs), vec!["fmt", "lint", "own"]);
        // les hooks importés tournent depuis le projet
        assert_eq!(cfgs.len(), 1);
    }

    #[test]
    fn hooks_file_cycle_is_an_error() {
        let dir = tempdir().unwrap();
        let root_path = dir.path().join(HOOK_CONFIG_FILENAME);
        write_json(
            &root_path,
            r#"{ "hooks": [ { "name": "loop", "path": "a.json" } ] }"#,
        );
        write_json(
            &dir.path().join("a.json"),
            r#"{ "hooks": [ { "name": "loop", "path": "a.json" } ] }"#,
        );

        let err = ConfigLoader::new().load_recursive(&root_path).unwrap_err();
        assert!(matches!(err, GoatHookerError::InvalidHook { .. }));
    }

    #[test]
    fn validate_from_and_path() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join(HOOK_CONFIG_FILENAME);

        for (json, valid) in [
            (r#"{ "name": "a", "from": "https://x/y.git" }"#, false),
            (r#"{ "name": "a", "path": "x.json", "cmd": "true" }"#, false),
            (r#"{ "name": "a", "path": "x.json", "rev": "main" }"#, false),
            (r#"{ "name": "a", "cmd": "true" }"#, false),
            (r#"{ "name": "a", "path": "x.json" }"#, true),
            (
                r#"{ "name": "a", "from": "https://x/y.git", "rev": "v1", "path": "x.json" }"#,
                true,
            ),
        ] {
            write_json(&config_path, &format!(r#"{{ "hooks": [ {json} ] }}"#));
            assert_eq!(
                Config::load_from_file(&config_path).is_ok(),
                valid,
                "{json}"
            );
        }
    }

    #[test]
    fn load_hooks_from_git_repo() {
        let dir = tempdir().unwrap();
        let cache = dir.path().join("cache");
        let shared = dir.path().join("shared");
        fs::create_dir_all(&shared).unwrap();

        let git = |args: &[&str]| {
            let status = std::process::Command::new("git")
                .args(args)
                .current_dir(&shared)
                .status()
                .unwrap();
            assert!(status.success());
        };
        git(&["init", "-q", "-b", "main"]);
        git(&["config", "user.name", "test"]);
        git(&["config", "user.email", "test@test"]);
        git(&["config", "commit.gpgsign", "false"]);
        write_json(
            &shared.join("rust.json"),
            r#"{ "hooks": [ { "name": "remote fmt", "trigger": "pre-commit", "cmd": "true" } ] }"#,
        );
        git(&["add", "."]);
        git(&["commit", "-q", "-m", "init"]);

        let url = format!("file://{}", shared.display());
        let project = dir.path().join("project");
        fs::create_dir_all(&project).unwrap();
        let root_path = project.join(HOOK_CONFIG_FILENAME);
        write_json(
            &root_path,
            &format!(
                r#"{{ "hooks": [ {{ "name": "shared", "from": "{url}", "rev": "main", "path": "rust.json" }} ] }}"#
            ),
        );

        // SAFETY: seul ce test modifie HOOKER_CACHE_DIR
        unsafe { std::env::set_var("HOOKER_CACHE_DIR", &cache) };

        let cfgs = ConfigLoader::new().load_recursive(&root_path).unwrap();
        assert_eq!(hook_names(&cfgs), vec!["remote fmt"]);

        // mise à jour du repo distant → visible seulement avec refresh
        write_json(
            &shared.join("rust.json"),
            r#"{ "hooks": [ { "name": "remote fmt v2", "trigger": "pre-commit", "cmd": "true" } ] }"#,
        );
        git(&["commit", "-q", "-am", "v2"]);

        let cfgs = ConfigLoader::new().load_recursive(&root_path).unwrap();
        assert_eq!(hook_names(&cfgs), vec!["remote fmt"]);

        let cfgs = ConfigLoader::new()
            .refresh_remotes()
            .load_recursive(&root_path)
            .unwrap();
        assert_eq!(hook_names(&cfgs), vec!["remote fmt v2"]);
    }

    #[test]
    fn schema_file_is_up_to_date() {
        // Comparé comme JSON : l'ordre des clés dépend des features de schemars
        let expected: serde_json::Value = serde_json::from_str(&json_schema()).unwrap();
        let current: serde_json::Value =
            serde_json::from_str(include_str!("../../schema.json")).unwrap();
        assert_eq!(
            current, expected,
            "schema.json is outdated, run `cargo run -- schema > schema.json`"
        );
    }
}
