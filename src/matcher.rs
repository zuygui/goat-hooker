// src/matcher.rs
use glob::{MatchOptions, Pattern};
use std::path::{Path, PathBuf};

use crate::config::{Config, Hook, Trigger};
use crate::errors::GoatHookerError;

/// `*` ne doit pas traverser les dossiers (`src/*.rs` ne matche pas `src/a/b.rs`)
const MATCH_OPTIONS: MatchOptions = MatchOptions {
    case_sensitive: true,
    require_literal_separator: true,
    require_literal_leading_dot: false,
};

/// Un hook avec ses patterns déjà compilés
struct CompiledHook<'a> {
    hook: &'a Hook,
    base_dir: PathBuf,
    include: Vec<Pattern>,
    exclude: Vec<Pattern>,
}

/// Un hook qui doit être exécuté, avec le dossier depuis lequel le lancer
#[derive(Debug)]
pub struct MatchedHook<'a> {
    pub hook: &'a Hook,
    pub base_dir: PathBuf,
    /// Fichiers concernés, relatifs à `base_dir` (vide si le trigger n'a pas de fichiers)
    pub files: Vec<PathBuf>,
}

pub struct Matcher<'a> {
    hooks: Vec<CompiledHook<'a>>,
}

impl<'a> Matcher<'a> {
    pub fn new(configs: &'a [Config]) -> Result<Self, GoatHookerError> {
        let mut hooks = Vec::new();

        for config in configs {
            let Some(base_dir) = config.base_dir.as_ref() else {
                continue;
            };
            let base_dir = normalize_path(base_dir);

            for hook in config.hooks.iter().flatten() {
                hooks.push(CompiledHook {
                    hook,
                    base_dir: base_dir.clone(),
                    include: compile_patterns(hook, hook.include.as_ref())?,
                    exclude: compile_patterns(hook, hook.exclude.as_ref())?,
                });
            }
        }

        Ok(Self { hooks })
    }

    /// Retourne tous les hooks qui matchent le fichier donné
    #[cfg(test)]
    pub fn match_hooks(&self, file_path: &Path) -> Vec<&'a Hook> {
        // Chemin absolu et normalisé du fichier
        let file_path_abs = normalize_path(file_path);

        self.hooks
            .iter()
            .filter(|compiled| compiled.matches(&file_path_abs))
            .map(|compiled| compiled.hook)
            .collect()
    }

    /// Retourne les hooks du trigger à exécuter.
    /// Si `files` est fourni, seuls les hooks qui matchent au moins un fichier sont gardés.
    pub fn hooks_to_run(
        &self,
        trigger: Trigger,
        files: Option<&[PathBuf]>,
    ) -> Vec<MatchedHook<'a>> {
        let files_abs: Option<Vec<PathBuf>> =
            files.map(|files| files.iter().map(|f| normalize_path(f)).collect());

        self.hooks
            .iter()
            .filter(|compiled| compiled.hook.trigger == Some(trigger))
            .filter_map(|compiled| {
                let matched_files = match files_abs.as_ref() {
                    Some(files) => {
                        let matched: Vec<PathBuf> = files
                            .iter()
                            .filter_map(|f| compiled.matched_path(f))
                            .collect();
                        if matched.is_empty() {
                            return None;
                        }
                        matched
                    }
                    None => vec![],
                };

                Some(MatchedHook {
                    hook: compiled.hook,
                    base_dir: compiled.base_dir.clone(),
                    files: matched_files,
                })
            })
            .collect()
    }
}

impl CompiledHook<'_> {
    /// Chemin relatif au projet si le fichier matche le hook
    fn matched_path(&self, file_path_abs: &Path) -> Option<PathBuf> {
        let relative = file_path_abs.strip_prefix(&self.base_dir).ok()?;
        self.matches_relative(relative)
            .then(|| relative.to_path_buf())
    }

    #[cfg(test)]
    fn matches(&self, file_path_abs: &Path) -> bool {
        self.matched_path(file_path_abs).is_some()
    }

    fn matches_relative(&self, relative: &Path) -> bool {
        // Vérifie si exclu avant même d’inclure
        if self
            .exclude
            .iter()
            .any(|pattern| pattern.matches_path_with(relative, MATCH_OPTIONS))
        {
            return false;
        }

        // Si pas d'include → hook sur tous les fichiers
        if self.include.is_empty() {
            return true;
        }

        self.include
            .iter()
            .any(|pattern| pattern.matches_path_with(relative, MATCH_OPTIONS))
    }
}

/// Compile les patterns glob d'un hook (relatifs au dossier du projet)
fn compile_patterns(
    hook: &Hook,
    patterns: Option<&Vec<String>>,
) -> Result<Vec<Pattern>, GoatHookerError> {
    patterns
        .into_iter()
        .flatten()
        .map(|pattern| {
            Pattern::new(pattern.trim_start_matches("./")).map_err(|source| {
                GoatHookerError::InvalidPattern {
                    hook: hook.name.clone(),
                    pattern: pattern.clone(),
                    source,
                }
            })
        })
        .collect()
}

/// Normalise un chemin (canonicalize si possible, sinon absolutize, sinon inchangé)
fn normalize_path(path: &Path) -> PathBuf {
    path.canonicalize()
        .or_else(|_| std::env::current_dir().map(|cwd| cwd.join(path)))
        .unwrap_or_else(|_| path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Config, Hook, Trigger};

    fn hook(name: &str, include: Option<Vec<&str>>, exclude: Option<Vec<&str>>) -> Hook {
        Hook {
            name: name.to_string(),
            trigger: Some(Trigger::PreCommit),
            include: include.map(|v| v.into_iter().map(String::from).collect()),
            exclude: exclude.map(|v| v.into_iter().map(String::from).collect()),
            cmd: Some(format!("echo {name}")),
            ..Default::default()
        }
    }

    #[test]
    fn test_match_hooks_with_exclude() {
        let hook1 = hook(
            "Rust files",
            Some(vec!["src/**/*.rs"]),
            Some(vec!["src/config/*.rs"]),
        );
        let hook2 = hook("Only config rs", Some(vec!["src/config/*.rs"]), None);
        let config = Config {
            hooks: Some(vec![hook1, hook2]),
            base_dir: Some(PathBuf::from(".")),
            ..Default::default()
        };

        let configs = [config];
        let matcher = Matcher::new(&configs).unwrap();

        // src/config/mod.rs doit matcher seulement hook2 (hook1 est exclu)
        let matched_hooks = matcher.match_hooks(Path::new("src/config/mod.rs"));
        assert_eq!(matched_hooks.len(), 1);
        assert_eq!(matched_hooks[0].name, "Only config rs");

        // src/main.rs doit matcher seulement hook1
        let matched_hooks = matcher.match_hooks(Path::new("src/main.rs"));
        assert_eq!(matched_hooks.len(), 1);
        assert_eq!(matched_hooks[0].name, "Rust files");

        // fichier inconnu → aucun hook
        let matched_hooks = matcher.match_hooks(Path::new("src/unknown.txt"));
        assert_eq!(matched_hooks.len(), 0);
    }

    #[test]
    fn star_does_not_cross_directories() {
        let config = Config {
            hooks: Some(vec![hook("Top level", Some(vec!["src/*.rs"]), None)]),
            base_dir: Some(PathBuf::from(".")),
            ..Default::default()
        };
        let configs = [config];
        let matcher = Matcher::new(&configs).unwrap();

        assert_eq!(matcher.match_hooks(Path::new("src/main.rs")).len(), 1);
        assert_eq!(matcher.match_hooks(Path::new("src/config/mod.rs")).len(), 0);
    }

    #[test]
    fn subproject_patterns_are_relative() {
        let dir = tempfile::tempdir().unwrap();
        let sub = dir.path().join("sub");
        std::fs::create_dir_all(sub.join("src")).unwrap();
        std::fs::write(sub.join("src/lib.rs"), "").unwrap();
        std::fs::write(dir.path().join("lib.rs"), "").unwrap();

        let config = Config {
            hooks: Some(vec![hook("Sub rust", Some(vec!["./src/*.rs"]), None)]),
            base_dir: Some(sub.clone()),
            ..Default::default()
        };
        let configs = [config];
        let matcher = Matcher::new(&configs).unwrap();

        assert_eq!(matcher.match_hooks(&sub.join("src/lib.rs")).len(), 1);

        // les fichiers passés au hook sont relatifs au sous-projet
        let files = [sub.join("src/lib.rs"), dir.path().join("lib.rs")];
        let hooks = matcher.hooks_to_run(Trigger::PreCommit, Some(&files));
        assert_eq!(hooks[0].files, vec![PathBuf::from("src/lib.rs")]);
        // fichier hors du sous-projet → ignoré
        assert_eq!(matcher.match_hooks(&dir.path().join("lib.rs")).len(), 0);
    }

    #[test]
    fn hooks_to_run_filters_by_trigger_and_files() {
        let mut push_hook = hook("Push", None, None);
        push_hook.trigger = Some(Trigger::PrePush);
        let config = Config {
            hooks: Some(vec![
                hook("Rust", Some(vec!["src/**/*.rs"]), None),
                push_hook,
            ]),
            base_dir: Some(PathBuf::from(".")),
            ..Default::default()
        };
        let configs = [config];
        let matcher = Matcher::new(&configs).unwrap();

        let files = [PathBuf::from("src/main.rs")];
        let hooks = matcher.hooks_to_run(Trigger::PreCommit, Some(&files));
        assert_eq!(hooks.len(), 1);
        assert_eq!(hooks[0].hook.name, "Rust");
        assert_eq!(hooks[0].files, vec![PathBuf::from("src/main.rs")]);

        let files = [PathBuf::from("README.md")];
        assert!(
            matcher
                .hooks_to_run(Trigger::PreCommit, Some(&files))
                .is_empty()
        );

        // pas de fichiers → tous les hooks du trigger
        let hooks = matcher.hooks_to_run(Trigger::PrePush, None);
        assert_eq!(hooks.len(), 1);
        assert_eq!(hooks[0].hook.name, "Push");
    }

    #[test]
    fn invalid_pattern_is_an_error() {
        let config = Config {
            hooks: Some(vec![hook("Broken", Some(vec!["src/[.rs"]), None)]),
            base_dir: Some(PathBuf::from(".")),
            ..Default::default()
        };
        let configs = [config];

        assert!(matches!(
            Matcher::new(&configs),
            Err(GoatHookerError::InvalidPattern { .. })
        ));
    }
}
