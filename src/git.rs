use std::{
    path::{Path, PathBuf},
    process::Command,
};

use crate::errors::GoatHookerError;

/// Lance une commande git dans `dir` et retourne sa sortie standard
fn git(dir: &Path, args: &[&str]) -> Result<String, GoatHookerError> {
    let output = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .map_err(|e| GoatHookerError::GitError(format!("failed to run git: {e}")))?;

    if !output.status.success() {
        return Err(GoatHookerError::GitError(
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ));
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Racine du dépôt
pub fn repo_root(dir: &Path) -> Result<PathBuf, GoatHookerError> {
    let root = git(dir, &["rev-parse", "--show-toplevel"])?;
    Ok(PathBuf::from(root.trim()))
}

/// Dossier des hooks (gère `core.hooksPath` et les worktrees)
pub fn hooks_dir(dir: &Path) -> Result<PathBuf, GoatHookerError> {
    let hooks = git(dir, &["rev-parse", "--git-path", "hooks"])?;
    let hooks = PathBuf::from(hooks.trim());

    if hooks.is_absolute() {
        Ok(hooks)
    } else {
        Ok(dir.join(hooks))
    }
}

/// Fichiers stagés (ajoutés, copiés, modifiés, renommés), en chemins absolus.
/// Les fichiers supprimés sont ignorés : il n'y a rien à vérifier dessus.
pub fn staged_files(root: &Path) -> Result<Vec<PathBuf>, GoatHookerError> {
    let output = git(
        root,
        &[
            "diff",
            "--cached",
            "--name-only",
            "--diff-filter=ACMR",
            "-z",
        ],
    )?;

    Ok(output
        .split('\0')
        .filter(|f| !f.is_empty())
        .map(|f| root.join(f))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    fn init_repo(dir: &Path) {
        git(dir, &["init", "-q"]).unwrap();
        git(dir, &["config", "user.name", "test"]).unwrap();
        git(dir, &["config", "user.email", "test@test"]).unwrap();
    }

    #[test]
    fn list_staged_files() {
        let dir = tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        init_repo(&root);

        fs::write(root.join("a.rs"), "").unwrap();
        fs::write(root.join("b.rs"), "").unwrap();
        fs::write(root.join("with space.rs"), "").unwrap();
        git(&root, &["add", "a.rs", "with space.rs"]).unwrap();

        let mut files = staged_files(&root).unwrap();
        files.sort();

        assert_eq!(files, vec![root.join("a.rs"), root.join("with space.rs")]);
    }

    #[test]
    fn ignore_deleted_files() {
        let dir = tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        init_repo(&root);

        fs::write(root.join("a.rs"), "").unwrap();
        git(&root, &["add", "a.rs"]).unwrap();
        git(&root, &["commit", "-q", "-m", "init"]).unwrap();
        git(&root, &["rm", "-q", "a.rs"]).unwrap();

        assert!(staged_files(&root).unwrap().is_empty());
    }

    #[test]
    fn find_hooks_dir() {
        let dir = tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        init_repo(&root);

        assert_eq!(hooks_dir(&root).unwrap(), root.join(".git/hooks"));

        git(&root, &["config", "core.hooksPath", ".githooks"]).unwrap();
        assert_eq!(hooks_dir(&root).unwrap(), root.join(".githooks"));
    }
}
