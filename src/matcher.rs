// src/matcher.rs
use glob::{MatchOptions, Pattern};
use std::path::{Path, PathBuf};

use crate::config::{Config, Hook, Trigger};
use crate::errors::GoatHookerError;

/// `*` must not cross folders (`src/*.rs` doesn't match `src/a/b.rs`)
const MATCH_OPTIONS: MatchOptions = MatchOptions {
    case_sensitive: true,
    require_literal_separator: true,
    require_literal_leading_dot: false,
};

/// A hook with its patterns already compiled
struct CompiledHook<'a> {
    hook: &'a Hook,
    base_dir: PathBuf,
    include: Vec<Pattern>,
    exclude: Vec<Pattern>,
}

/// A hook to run, with the folder to run it from
#[derive(Debug)]
pub struct MatchedHook<'a> {
    pub hook: &'a Hook,
    pub base_dir: PathBuf,
    /// Targeted files, relative to `base_dir` (empty when the trigger has no files)
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

    /// Returns all the hooks matching the given file
    #[cfg(test)]
    pub fn match_hooks(&self, file_path: &Path) -> Vec<&'a Hook> {
        // Absolute and normalized path of the file
        let file_path_abs = normalize_path(file_path);

        self.hooks
            .iter()
            .filter(|compiled| compiled.matches(&file_path_abs))
            .map(|compiled| compiled.hook)
            .collect()
    }

    /// Returns the hooks of the trigger to run.
    /// When `files` is given, only the hooks matching at least one file are kept.
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
    /// Path relative to the project if the file matches the hook
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
        // Checks the exclusions before the inclusions
        if self
            .exclude
            .iter()
            .any(|pattern| pattern.matches_path_with(relative, MATCH_OPTIONS))
        {
            return false;
        }

        // Without include → hook on every file
        if self.include.is_empty() {
            return true;
        }

        self.include
            .iter()
            .any(|pattern| pattern.matches_path_with(relative, MATCH_OPTIONS))
    }
}

/// Compiles the glob patterns of a hook (relative to the folder of the project)
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

/// Normalizes a path (canonicalize if possible, else absolutize, else unchanged)
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

        // src/config/mod.rs must match only hook2 (hook1 excludes it)
        let matched_hooks = matcher.match_hooks(Path::new("src/config/mod.rs"));
        assert_eq!(matched_hooks.len(), 1);
        assert_eq!(matched_hooks[0].name, "Only config rs");

        // src/main.rs must match only hook1
        let matched_hooks = matcher.match_hooks(Path::new("src/main.rs"));
        assert_eq!(matched_hooks.len(), 1);
        assert_eq!(matched_hooks[0].name, "Rust files");

        // unknown file → no hook
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

        // the files given to the hook are relative to the sub-project
        let files = [sub.join("src/lib.rs"), dir.path().join("lib.rs")];
        let hooks = matcher.hooks_to_run(Trigger::PreCommit, Some(&files));
        assert_eq!(hooks[0].files, vec![PathBuf::from("src/lib.rs")]);
        // file outside the sub-project → ignored
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

        // no files → all the hooks of the trigger
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
