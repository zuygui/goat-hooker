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

/// A compiled glob. The usual shapes (`*.rs`, `**/*.rs`, `**`, a plain path) are checked
/// without the glob engine: a commit can have thousands of files for each hook.
enum Glob {
    /// `*.ext`: a file of the folder itself ending with `.ext`
    Suffix(String),
    /// `**/*.ext`: any file ending with `.ext`
    DeepSuffix(String),
    /// `**`, `**/*`: everything
    All,
    /// A path without wildcards
    Exact(String),
    Pattern(Pattern),
}

/// No character with a meaning in a glob
fn is_plain(text: &str) -> bool {
    !text.contains(['*', '?', '[', ']', '{', '}', '!', '\\'])
}

impl Glob {
    fn new(pattern: &str) -> Result<Self, glob::PatternError> {
        let compiled = Pattern::new(pattern)?;
        let suffix = |prefix: &str| {
            pattern
                .strip_prefix(prefix)
                .filter(|ext| is_plain(ext) && !ext.contains('/'))
                .map(String::from)
        };
        Ok(if pattern == "**" || pattern == "**/*" {
            Glob::All
        } else if let Some(ext) = suffix("**/*") {
            Glob::DeepSuffix(ext)
        } else if let Some(ext) = suffix("*") {
            Glob::Suffix(ext)
        } else if is_plain(pattern) {
            Glob::Exact(pattern.to_string())
        } else {
            Glob::Pattern(compiled)
        })
    }

    /// `path`: relative to the project, with `/` between folders
    fn matches(&self, path: &str, original: &Path) -> bool {
        match self {
            Glob::Suffix(ext) => !path.contains('/') && path.ends_with(ext.as_str()),
            Glob::DeepSuffix(ext) => path
                .rsplit('/')
                .next()
                .is_some_and(|name| name.ends_with(ext.as_str())),
            Glob::All => true,
            Glob::Exact(exact) => path == exact,
            Glob::Pattern(pattern) => pattern.matches_path_with(original, MATCH_OPTIONS),
        }
    }
}

/// A hook with its patterns already compiled
struct CompiledHook<'a> {
    hook: &'a Hook,
    base_dir: PathBuf,
    include: Vec<Glob>,
    exclude: Vec<Glob>,
}

/// A hook to run, with the folder to run it from
#[derive(Debug)]
pub struct MatchedHook<'a> {
    pub hook: &'a Hook,
    pub base_dir: PathBuf,
    /// Targeted files, relative to `base_dir` (empty when the trigger has no files)
    pub files: Vec<PathBuf>,
    /// Runs because a project it depends on changed (`affected`), not for its own files
    pub affected_by: Option<PathBuf>,
}

pub struct Matcher<'a> {
    hooks: Vec<CompiledHook<'a>>,
    dependencies: crate::deps::Dependencies,
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

        Ok(Self {
            hooks,
            dependencies: crate::deps::Dependencies::detect(configs),
        })
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
        let files_abs: Option<Vec<PathBuf>> = files.map(normalize_files);
        // Only the `affected` hooks look at the projects that changed
        let changed_projects = match files_abs.as_ref() {
            Some(files) if self.hooks.iter().any(|c| c.hook.affected == Some(true)) => {
                self.dependencies.changed_projects(files)
            }
            _ => Default::default(),
        };

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
                            // A project it depends on changed
                            if compiled.hook.affected == Some(true)
                                && let Some(changed) = self
                                    .dependencies
                                    .affected_by(&compiled.base_dir, &changed_projects)
                            {
                                return Some(MatchedHook {
                                    hook: compiled.hook,
                                    base_dir: compiled.base_dir.clone(),
                                    files: vec![],
                                    affected_by: Some(changed),
                                });
                            }
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
                    affected_by: None,
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
        let text = relative.to_string_lossy();
        // `\` on Windows: the globs use `/`
        let text = if cfg!(windows) {
            std::borrow::Cow::Owned(text.replace('\\', "/"))
        } else {
            text
        };
        // Checks the exclusions before the inclusions
        if self
            .exclude
            .iter()
            .any(|glob| glob.matches(&text, relative))
        {
            return false;
        }

        // Without include → hook on every file
        if self.include.is_empty() {
            return true;
        }

        self.include
            .iter()
            .any(|glob| glob.matches(&text, relative))
    }
}

/// Compiles the glob patterns of a hook (relative to the folder of the project)
fn compile_patterns(
    hook: &Hook,
    patterns: Option<&Vec<String>>,
) -> Result<Vec<Glob>, GoatHookerError> {
    patterns
        .into_iter()
        .flatten()
        .map(|pattern| {
            Glob::new(pattern.trim_start_matches("./")).map_err(|source| {
                GoatHookerError::InvalidPattern {
                    hook: hook.name.clone(),
                    pattern: pattern.clone(),
                    source,
                }
            })
        })
        .collect()
}

/// Normalizes many files: the folders are resolved once each (a commit can have thousands of
/// files in a few folders, and resolving a path costs a system call per component)
pub(crate) fn normalize_files(files: &[PathBuf]) -> Vec<PathBuf> {
    let mut folders: std::collections::HashMap<&Path, PathBuf> = std::collections::HashMap::new();
    files
        .iter()
        .map(|file| match (file.parent(), file.file_name()) {
            (Some(parent), Some(name)) if file.is_absolute() => folders
                .entry(parent)
                .or_insert_with(|| normalize_path(parent))
                .join(name),
            _ => normalize_path(file),
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

    #[test]
    fn fast_globs_match_like_the_glob_engine() {
        let patterns = [
            "*.rs",
            "**/*.rs",
            "**",
            "**/*",
            "src/main.rs",
            "*.test.ts",
            "**/*.d.ts",
            "Cargo.toml",
            "src/*.rs",
            "**/src/*.rs",
            "*",
        ];
        let paths = [
            "main.rs",
            "src/main.rs",
            "src/a/b.rs",
            "a.test.ts",
            "web/a.test.ts",
            "types/x.d.ts",
            "x.d.ts",
            "Cargo.toml",
            "crates/a/Cargo.toml",
            ".rs",
            "rs",
            "README.md",
            "a.rsx",
        ];
        for pattern in patterns {
            let fast = Glob::new(pattern).unwrap();
            let engine = Pattern::new(pattern).unwrap();
            for path in paths {
                assert_eq!(
                    fast.matches(path, Path::new(path)),
                    engine.matches_path_with(Path::new(path), MATCH_OPTIONS),
                    "{pattern} on {path}"
                );
            }
        }
    }
}
