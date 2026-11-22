//! Recherche des dépôts git sur l'ordinateur (page d'accueil)

use std::{
    fs,
    path::{Path, PathBuf},
    time::SystemTime,
};

use goat_hooker::config::HOOK_CONFIG_FILENAME;

/// Profondeur max sous un dossier scanné (`~/dev/org/repo` = 3)
const MAX_DEPTH: usize = 4;
/// Nombre max de dossiers visités par scan (évite de parcourir tout le disque)
const MAX_VISITED: usize = 40_000;

/// Dossiers jamais parcourus : dépendances, builds, et dossiers protégés de macOS
/// (y accéder afficherait une demande d'autorisation au lancement)
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
    /// A déjà un `hooker.config.json`
    pub configured: bool,
    /// Dernière activité git (pour trier)
    pub modified: SystemTime,
}

/// Dossiers scannés par défaut : le dossier personnel
pub fn default_roots() -> Vec<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .into_iter()
        .collect()
}

/// Cherche les dépôts git sous `roots`.
/// Un dossier choisi par l'utilisateur est parcouru même s'il est dans `SKIPPED`.
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

    // Un dépôt : on le garde, sans descendre dedans (sous-modules, monorepo)
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
        init(&home.join("dev/side-project/sub")); // dans un dépôt : ignoré
        init(&home.join("dev/node_modules/pkg")); // dépendances : ignoré
        init(&home.join("Documents/thesis")); // protégé sur macOS : ignoré
        init(&home.join(".cache/repo")); // caché : ignoré
        init(&home.join("a/b/c/d/e/too-deep"));
        fs::write(home.join("dev/side-project/hooker.config.json"), "{}").unwrap();

        let found = find_repositories(&[home.to_path_buf()]);
        let paths: Vec<PathBuf> = found.iter().map(|r| r.path.clone()).collect();
        assert_eq!(
            paths,
            vec![home.join("dev/side-project"), home.join("dev/org/api")]
        );
        assert!(found[0].configured);

        // un dossier choisi est parcouru même s'il est protégé
        let found = find_repositories(&[home.join("Documents")]);
        assert_eq!(found[0].path, home.join("Documents/thesis"));
    }
}
