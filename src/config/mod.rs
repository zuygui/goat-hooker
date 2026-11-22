use std::{
    collections::{BTreeMap, HashSet},
    fs,
    path::{Path, PathBuf},
};

use clap::ValueEnum;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{errors::GoatHookerError, git, remote};

mod template;

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

    /// What a reusable hooks file does (shown by `hooker search`)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(description = "What this hooks file does, shown when searching hooks")]
    pub description: Option<String>,

    /// Root config only: this repository shares hooks files, found by `hooker search`
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(
        description = "Share hooks files of this repository: `hooker search` finds the root configs with this field on GitHub (root config only)"
    )]
    pub share: Option<Share>,

    /// Paramètres d'un fichier de hooks réutilisable (utilisés en `{{nom}}`)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(
        description = "Parameters of a reusable hooks file, used as `{{name}}` in its hooks and set with `with` by the hook loading it"
    )]
    pub params: Option<BTreeMap<String, Param>>,

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

    /// Values shared between hooks, kept between runs
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(
        description = "Variables shared between hooks: a hook declares the ones it `writes` (`echo \"name=value\" >> $HOOKER_VARS`) and `reads` (`$name`). Their values are kept between runs"
    )]
    pub variables: Option<BTreeMap<String, Variable>>,

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
    #[schemars(description = "Values of the `params` of the hooks file loaded with `path`")]
    pub with: Option<BTreeMap<String, serde_json::Value>>,

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

    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(
        description = "Variables this hook reads, available as `$name` (it runs after the hooks writing them)"
    )]
    pub reads: Option<Vec<String>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(
        description = "Variables this hook writes with `echo \"name=value\" >> $HOOKER_VARS`"
    )]
    pub writes: Option<Vec<String>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(
        description = "Skip the hook when it already passed with the same files, commands, inputs and variables. Default: true for hooks with `include`"
    )]
    pub cache: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(
        description = "What this hook is allowed to do. Without it, a hook of the repository can do everything and a hook loaded with `from` can only read files"
    )]
    pub permissions: Option<Permissions>,

    /// Set when the hook comes from another repository (`from`): it must be trusted to run
    #[serde(skip)]
    #[schemars(skip)]
    pub origin: Option<Origin>,
}

/// A variable shared between hooks
#[derive(Debug, Default, Serialize, Deserialize, Clone, JsonSchema)]
#[schemars(description = "A variable shared between hooks")]
pub struct Variable {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(description = "What the variable holds")]
    pub description: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(description = "Value before any hook writes it")]
    pub default: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(
        description = "Hooks allowed to write it: `name`, `project:name` or `*` (default: every hook)"
    )]
    pub writers: Option<Vec<String>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(
        description = "Hooks allowed to read it: `name`, `project:name` or `*` (default: every hook)"
    )]
    pub readers: Option<Vec<String>>,
}

/// Access of a hook to the files of the repository
#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum FileAccess {
    /// The files may not be modified (checked after the hook, changes are reverted)
    Read,
    /// The hook may modify files (formatters)
    Write,
}

/// What a hook is allowed to do
#[derive(Debug, Default, Serialize, Deserialize, Clone, PartialEq, Eq, JsonSchema)]
#[schemars(description = "What a hook is allowed to do")]
pub struct Permissions {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(
        description = "`read`: the targeted files can't be modified (changes are reverted and the hook fails), `write`: they can"
    )]
    pub files: Option<FileAccess>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(
        description = "Network access. When false, it is blocked where the system allows it (Linux user namespaces, macOS sandbox)"
    )]
    pub network: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(
        description = "Environment variables the hook gets, besides the basic ones (PATH, HOME, GIT_*...). Without it, a hook of the repository gets all of them"
    )]
    pub env: Option<Vec<String>>,
}

/// Permissions of a hook once the defaults are applied
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Granted {
    pub files: FileAccess,
    pub network: bool,
    /// `None`: the whole environment
    pub env: Option<Vec<String>>,
}

/// Where a hook loaded from another repository comes from
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Origin {
    pub url: String,
    pub rev: Option<String>,
    /// Hooks file in that repository
    pub path: String,
    /// Hash of the hooks file (and of the files it loads) and of the `with` values:
    /// changes when the hooks change
    pub hash: String,
    /// The hooks file in the cache
    pub file: PathBuf,
    /// `with` values, as JSON
    pub with: String,
    /// Commit of the repository in the cache
    pub commit: Option<String>,
}

impl Origin {
    /// Key in the trust store (the same for every version of the file)
    pub fn key(&self) -> String {
        format!("{}:{}", self.url, self.path)
    }
}

/// Hooks files shared by a repository
#[derive(Debug, Default, Serialize, Deserialize, Clone, JsonSchema)]
#[schemars(description = "Hooks files shared by this repository")]
pub struct Share {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(description = "What these hooks are for")]
    pub description: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(description = "Keywords to find them (rust, lint, commits...)")]
    pub tags: Option<Vec<String>>,

    #[schemars(
        description = "Shared hooks files, relative to the repository root. Globs are supported (hooks/*.json)"
    )]
    pub hooks: Vec<String>,
}

// Paramètre d'un fichier de hooks réutilisable
#[derive(Debug, Default, Serialize, Deserialize, Clone, JsonSchema)]
#[schemars(description = "A parameter of a reusable hooks file")]
pub struct Param {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(description = "What the parameter is for")]
    pub description: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(description = "Value used when `with` doesn't set it (required without it)")]
    pub default: Option<serde_json::Value>,
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

    /// Permissions with the defaults: everything for the hooks of the repository,
    /// read-only files, no network and the basic environment for the hooks of other repositories
    pub fn granted(&self) -> Granted {
        let remote = self.origin.is_some();
        let permissions = self.permissions.clone().unwrap_or_default();
        Granted {
            files: permissions.files.unwrap_or(if remote {
                FileAccess::Read
            } else {
                FileAccess::Write
            }),
            network: permissions.network.unwrap_or(!remote),
            env: match permissions.env {
                Some(env) => Some(env),
                None if remote => Some(vec![]),
                None => None,
            },
        }
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
        if self.with.is_some() && self.path.is_none() {
            return Err(invalid("`with` requires `path`"));
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
        for name in self.reads.iter().chain(self.writes.iter()).flatten() {
            if !is_identifier(name) {
                return Err(invalid(&format!(
                    "variable `{name}` must be a valid shell variable name"
                )));
            }
        }
        if self.stage_fixed == Some(true) && self.granted().files == FileAccess::Read {
            return Err(invalid(
                "`stage_fixed` needs the permission to modify files (`\"permissions\": { \"files\": \"write\" }`)",
            ));
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
        Self::load_with(path, None, "")
    }

    /// Charge un fichier en remplissant ses `params` avec `with` (`importer` : hook qui le charge)
    pub fn load_with(
        path: &Path,
        with: Option<&BTreeMap<String, serde_json::Value>>,
        importer: &str,
    ) -> Result<Config, GoatHookerError> {
        let content = fs::read_to_string(path).map_err(GoatHookerError::ReadError)?;
        let parse_error = |source| GoatHookerError::ParseError {
            path: path.to_path_buf(),
            source,
        };
        let mut value: serde_json::Value = serde_json::from_str(&content).map_err(parse_error)?;

        // Paramètres : remplacés dans les hooks avant de les lire
        if let Some(params) = value.get("params").cloned() {
            let params: BTreeMap<String, Param> =
                serde_json::from_value(params).map_err(parse_error)?;
            if let Some(hooks) = value.get_mut("hooks") {
                template::apply(hooks, &params, with).map_err(|reason| {
                    GoatHookerError::InvalidHook {
                        name: if importer.is_empty() {
                            path.display().to_string()
                        } else {
                            importer.to_string()
                        },
                        reason,
                    }
                })?;
            }
        } else if let Some((name, _)) = with.and_then(|w| w.iter().next()) {
            return Err(GoatHookerError::InvalidHook {
                name: importer.to_string(),
                reason: format!(
                    "`with` sets `{name}`, but {} has no `params`",
                    path.display()
                ),
            });
        }

        let mut cfg: Config = serde_json::from_value(value).map_err(parse_error)?;

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
            root.hooks = Some(self.expand_hooks(hooks, &base_dir, 0, None)?);
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
        origin: Option<&Origin>,
    ) -> Result<Vec<Hook>, GoatHookerError> {
        let mut expanded = Vec::new();

        for hook in hooks {
            let Some(path) = hook.path.as_ref() else {
                let mut hook = hook;
                if let Some(origin) = origin {
                    hook.origin = Some(origin.clone());
                    // The defaults of the permissions changed
                    hook.validate()?;
                }
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

            let shared = Config::load_with(&file, hook.with.as_ref(), &hook.name)?;
            let shared_dir = shared.base_dir.clone().unwrap();
            let shared_hooks = shared.hooks.unwrap_or_default();

            // Hooks of another repository: they must be trusted, and run with restricted permissions
            let remote_origin = match (origin, hook.from.as_ref()) {
                (Some(origin), _) => Some(origin.clone()),
                (None, Some(url)) => Some(Origin {
                    url: url.clone(),
                    rev: hook.rev.clone(),
                    path: path.clone(),
                    hash: content_hash(&file, hook.with.as_ref())?,
                    file: file.clone(),
                    with: serde_json::to_string(&hook.with).unwrap_or_default(),
                    commit: remote::head(file.parent().unwrap_or(&file)),
                }),
                (None, None) => None,
            };

            // Un fichier de hooks peut lui-même charger des fichiers (relatifs à lui)
            expanded.extend(self.expand_hooks(
                shared_hooks,
                &shared_dir,
                depth + 1,
                remote_origin.as_ref(),
            )?);
        }

        Ok(expanded)
    }
}

/// Hash of a hooks file and of the values given to its parameters
fn content_hash(
    file: &Path,
    with: Option<&BTreeMap<String, serde_json::Value>>,
) -> Result<String, GoatHookerError> {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hash_file(&mut hasher, file, 0)?;
    hasher.update(b"\0");
    hasher.update(serde_json::to_string(&with).unwrap_or_default());
    Ok(hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}

/// Adds a hooks file and the files it loads with `path` to a hash
fn hash_file(hasher: &mut sha2::Sha256, file: &Path, depth: usize) -> Result<(), GoatHookerError> {
    use sha2::Digest;
    let content = fs::read(file)?;
    hasher.update((content.len() as u64).to_le_bytes());
    hasher.update(&content);
    if depth >= MAX_HOOKS_FILES_DEPTH {
        return Ok(());
    }
    let value: serde_json::Value = serde_json::from_slice(&content).unwrap_or_default();
    let dir = file.parent().unwrap_or(Path::new("."));
    for hook in value["hooks"].as_array().into_iter().flatten() {
        if let (Some(path), None) = (hook["path"].as_str(), hook["from"].as_str()) {
            hash_file(hasher, &dir.join(path), depth + 1)?;
        }
    }
    Ok(())
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

    #[test]
    fn load_reusable_hooks_with_params() {
        let dir = tempdir().unwrap();
        let root_path = dir.path().join(HOOK_CONFIG_FILENAME);
        write_json(
            &root_path,
            r#"{ "hooks": [
                { "name": "front lint", "path": "eslint.json", "with": { "files": ["web/**/*.ts"], "name": "front" } },
                { "name": "default lint", "path": "eslint.json", "with": { "files": "*.ts" } }
            ] }"#,
        );
        write_json(
            &dir.path().join("eslint.json"),
            r#"{
                "params": { "files": {}, "name": { "default": "lint" }, "config": { "default": ".eslintrc.json" } },
                "hooks": [ { "name": "eslint {{name}}", "trigger": "pre-commit", "include": ["{{files}}"],
                             "cmd": "eslint -c {{config}} {files}" } ]
            }"#,
        );

        let cfgs = ConfigLoader::new().load_recursive(&root_path).unwrap();
        let hooks = cfgs[0].hooks.clone().unwrap();
        assert_eq!(hooks[0].name, "eslint front");
        assert_eq!(hooks[0].include, Some(vec!["web/**/*.ts".to_string()]));
        assert_eq!(
            hooks[0].cmd.as_deref(),
            Some("eslint -c .eslintrc.json {files}")
        );
        assert_eq!(hooks[1].name, "eslint lint");

        // paramètre requis manquant
        write_json(
            &root_path,
            r#"{ "hooks": [ { "name": "lint", "path": "eslint.json" } ] }"#,
        );
        let err = ConfigLoader::new().load_recursive(&root_path).unwrap_err();
        assert!(
            err.to_string().contains("parameter `files` is required"),
            "{err}"
        );
    }
}
