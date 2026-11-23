//! Search of the git repositories of the computer (welcome page)

use std::{
    fs,
    path::{Path, PathBuf},
    time::SystemTime,
};

use goat_hooker::config::HOOK_CONFIG_FILENAME;

/// Max depth under a scanned folder (`~/dev/org/repo` = 3)
const MAX_DEPTH: usize = 4;
/// Max number of folders visited by a scan (avoids walking the whole disk)
const MAX_VISITED: usize = 40_000;

/// Folders never walked: dependencies, builds, and protected folders of macOS
/// (reading them would show an authorization prompt at launch)
const SKIPPED: &[&str] = &[
    "node_modules",
    "target",
    "vendor",
    "build",
    "dist",
    "Library",
    "Applications",
    "Desktop",
    "Documents",
    "Downloads",
    "Movies",
    "Music",
    "Pictures",
    "Public",
    "AppData",
    "snap",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FoundRepo {
    pub path: PathBuf,
    /// Already has a `hooker.config.json`
    pub configured: bool,
    /// Last git activity (to sort)
    pub modified: SystemTime,
}

/// Folders scanned by default: the home folder
pub fn default_roots() -> Vec<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .into_iter()
        .collect()
}

/// Searches the git repositories under `roots`.
/// A folder picked by the user is walked even if it is in `SKIPPED`.
pub fn find_repositories(roots: &[PathBuf]) -> Vec<FoundRepo> {
    let mut found = Vec::new();
    let mut visited = 0;
    for root in roots {
        scan(root, 0, &mut found, &mut visited);
    }

    found.sort_by(|a, b| {
        b.configured
            .cmp(&a.configured)
            .then(b.modified.cmp(&a.modified))
    });
    found.dedup_by(|a, b| a.path == b.path);
    found
}

fn scan(dir: &Path, depth: usize, found: &mut Vec<FoundRepo>, visited: &mut usize) {
    if *visited >= MAX_VISITED {
        return;
    }
    *visited += 1;

    // A repository: kept, without going into it (submodules, monorepo)
    let git = dir.join(".git");
    if git.exists() {
        let modified = ["index", "HEAD"]
            .iter()
            .filter_map(|f| fs::metadata(git.join(f)).and_then(|m| m.modified()).ok())
            .max()
            .unwrap_or(SystemTime::UNIX_EPOCH);
        found.push(FoundRepo {
            path: dir.to_path_buf(),
            configured: dir.join(HOOK_CONFIG_FILENAME).exists(),
            modified,
        });
        return;
    }

    if depth >= MAX_DEPTH {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut children: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .map(|e| e.path())
        .filter(|p| {
            let name = p.file_name().unwrap_or_default().to_string_lossy();
            !name.starts_with('.') && !SKIPPED.contains(&name.as_ref())
        })
        .collect();
    children.sort();
    for child in children {
        scan(&child, depth + 1, found, visited);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    fn init(dir: &Path) {
        fs::create_dir_all(dir).unwrap();
        Command::new("git")
            .args(["init", "-q"])
            .current_dir(dir)
            .status()
            .unwrap();
    }

    #[test]
    fn find_repositories_under_a_folder() {
        let home = tempfile::tempdir().unwrap();
        let home = home.path();
        init(&home.join("dev/org/api"));
        init(&home.join("dev/side-project"));
        init(&home.join("dev/side-project/sub")); // in a repository: ignored
        init(&home.join("dev/node_modules/pkg")); // dependencies: ignored
        init(&home.join("Documents/thesis")); // protected on macOS: ignored
        init(&home.join(".cache/repo")); // hidden: ignored
        init(&home.join("a/b/c/d/e/too-deep"));
        fs::write(home.join("dev/side-project/hooker.config.json"), "{}").unwrap();

        let found = find_repositories(&[home.to_path_buf()]);
        let paths: Vec<PathBuf> = found.iter().map(|r| r.path.clone()).collect();
        assert_eq!(
            paths,
            vec![home.join("dev/side-project"), home.join("dev/org/api")]
        );
        assert!(found[0].configured);

        // a picked folder is walked even if it is protected
        let found = find_repositories(&[home.join("Documents")]);
        assert_eq!(found[0].path, home.join("Documents/thesis"));
    }
}
