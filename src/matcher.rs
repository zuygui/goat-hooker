// src/matcher.rs
use glob::Pattern;
use std::path::{Path, PathBuf};

use crate::config::{Config, Hook};

pub struct Matcher<'a> {
    configs: &'a [Config],
}

impl<'a> Matcher<'a> {
    pub fn new(configs: &'a [Config]) -> Self {
        Self { configs }
    }

    /// Retourne tous les hooks qui matchent le fichier donné
    pub fn match_hooks(&self, file_path: &Path) -> Vec<&'a Hook> {
        let mut matched_hooks: Vec<&'a Hook> = Vec::new();

        // Chemin absolu et normalisé du fichier
        let file_path_abs = normalize_path(file_path);

        for config in self.configs {
            let Some(base_dir) = config.base_dir.as_ref() else {
                continue;
            };
            let base_dir_abs = normalize_path(base_dir);

            // Vérifie que le fichier est bien dans le projet
            if !file_path_abs.starts_with(&base_dir_abs) {
                continue;
            }

            for hooks in config.hooks.iter() {
                for hook in hooks.iter() {
                    // Vérifie si exclu avant même d’inclure
                    if is_excluded(hook, &file_path_abs, &base_dir_abs) {
                        continue;
                    }

                    // Si pas d'include → hook sur tous les fichiers
                    if hook.include.as_ref().map_or(true, |v| v.is_empty()) {
                        matched_hooks.push(hook);
                        continue;
                    }

                    // Sinon on check include
                    if let Some(includes) = hook.include.as_ref() {
                        if includes
                            .iter()
                            .any(|pattern| matches_glob(pattern, &file_path_abs, &base_dir_abs))
                        {
                            matched_hooks.push(hook);
                        }
                    }
                }
            }
        }

        matched_hooks
    }
}

/// Vérifie si un fichier est exclu par le hook
fn is_excluded(hook: &Hook, file_path: &Path, base_dir: &Path) -> bool {
    if let Some(excludes) = hook.exclude.as_ref() {
        excludes
            .iter()
            .any(|pattern| matches_glob(pattern, file_path, base_dir))
    } else {
        false
    }
}

/// Vérifie un pattern glob vs fichier
fn matches_glob(pattern: &str, file_path: &Path, base_dir: &Path) -> bool {
    let full_pattern = base_dir.join(pattern);
    let Some(full_pattern_str) = full_pattern.to_str() else {
        return false;
    };

    let Ok(glob_pattern) = Pattern::new(full_pattern_str) else {
        return false;
    };
    glob_pattern.matches_path(file_path)
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

    #[test]
    fn test_match_hooks_with_exclude() {
        let hook = Hook {
            name: "Rust files".to_string(),
            trigger: Trigger::PreCommit,
            include: Some(vec!["src/**/*.rs".to_string()]),
            exclude: Some(vec!["src/config/*.rs".to_string()]),
            cmd: Some("echo Rust file".to_string()),
            cmds: None,
            from: None,
            path: None,
        };
        let hook2 = Hook {
            name: "Only config rs".to_string(),
            trigger: Trigger::PreCommit,
            include: Some(vec!["src/config/*.rs".to_string()]),
            exclude: None,
            cmd: Some("echo Config Rust file".to_string()),
            cmds: None,
            from: None,
            path: None,
        };
        let config = Config {
            imports: None,
            hooks: Some(vec![hook, hook2]),
            base_dir: Some(PathBuf::from(".")),
        };

        let configs = [config];
        let matcher = Matcher::new(&configs);

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
}
