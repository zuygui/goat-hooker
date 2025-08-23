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
            // Base dir du projet
            let Some(base_dir) = config.base_dir.as_ref() else {
                continue;
            };
            let base_dir_abs = normalize_path(base_dir);

            // Vérifie que le fichier est bien dans le sous-projet
            if !file_path_abs.starts_with(&base_dir_abs) {
                continue;
            }

            for hooks in config.hooks.iter() {
                for hook in hooks.iter() {
                    // Si pas de "include", le hook s'applique à tous les fichiers
                    if hook.include.as_ref().map_or(true, |v| v.is_empty()) {
                        matched_hooks.push(hook);
                        continue;
                    }

                    // Sinon on check chaque pattern glob
                    if let Some(includes) = hook.include.as_ref() {
                        for pattern in includes {
                            let full_pattern = base_dir_abs.join(pattern);
                            let full_pattern_str = match full_pattern.to_str() {
                                Some(s) => s,
                                None => continue, // skip si chemin invalide UTF-8
                            };

                            // Construire le pattern glob
                            let glob_pattern = match Pattern::new(full_pattern_str) {
                                Ok(p) => p,
                                Err(_) => continue, // skip si pattern invalide
                            };

                            if glob_pattern.matches_path(&file_path_abs) {
                                matched_hooks.push(hook);
                                break; // un match suffit
                            }
                        }
                    }
                }
            }
        }

        matched_hooks
    }
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
    fn test_match_hooks() {
        let hook = Hook {
            name: "Rust files".to_string(),
            trigger: Trigger::PreCommit,
            include: Some(vec!["src/**/*.rs".to_string()]),
            cmd: Some("echo Rust file".to_string()),
            cmds: None,
            from: None,
            path: None,
        };
        let hook2 = Hook {
            name: "Config rust files".to_string(),
            trigger: Trigger::PreCommit,
            include: Some(vec!["src/config/*.rs".to_string()]),
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

        // match mod.rs -> doit matcher les deux hooks
        let matched_hooks = matcher.match_hooks(Path::new("src/config/mod.rs"));
        assert_eq!(matched_hooks.len(), 2);
        assert!(matched_hooks.iter().any(|h| h.name == "Rust files"));
        assert!(matched_hooks.iter().any(|h| h.name == "Config rust files"));

        // match main.rs -> doit matcher seulement "Rust files"
        let matched_hooks = matcher.match_hooks(Path::new("src/main.rs"));
        assert_eq!(matched_hooks.len(), 1);
        assert_eq!(matched_hooks[0].name, "Rust files");

        // fichier qui ne correspond à aucun pattern
        let matched_hooks = matcher.match_hooks(Path::new("src/unknown.txt"));
        assert_eq!(matched_hooks.len(), 0);
    }
}
