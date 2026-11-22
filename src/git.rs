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

    Ok(split_paths(root, &output))
}

/// Fichiers modifiés par les commits poussés (ajoutés, copiés, modifiés, renommés).
/// `refs` est ce que Git donne au pre-push sur stdin :
/// `<local ref> <local sha> <remote ref> <remote sha>` (une ligne par ref).
pub fn pushed_files(
    root: &Path,
    remote: &str,
    refs: &str,
) -> Result<Vec<PathBuf>, GoatHookerError> {
    // Sans remote nommé (push vers une URL), on compare à toutes les branches distantes
    let not_remotes = if remote.is_empty() || remote.contains(['/', ':']) {
        "--remotes".to_string()
    } else {
        format!("--remotes={remote}")
    };

    let mut files = Vec::new();
    for line in refs.lines() {
        let mut parts = line.split_whitespace();
        let (Some(_local_ref), Some(local_sha)) = (parts.next(), parts.next()) else {
            continue;
        };

        // Suppression d'une branche distante → rien à vérifier
        if local_sha.chars().all(|c| c == '0') {
            continue;
        }

        // Commits qui ne sont pas encore sur le remote
        let output = git(
            root,
            &[
                "log",
                "--format=",
                "--name-only",
                "--diff-filter=ACMR",
                "-z",
                local_sha,
                "--not",
                &not_remotes,
            ],
        )?;
        files.extend(split_paths(root, &output));
    }

    files.sort();
    files.dedup();
    Ok(files)
}

/// Fichiers nommés `name` suivis ou non ignorés par git, sous `dir`
pub fn find_files(dir: &Path, name: &str) -> Result<Vec<PathBuf>, GoatHookerError> {
    let pathspec = format!(":(glob)**/{name}");
    let output = git(
        dir,
        &[
            "ls-files",
            "--cached",
            "--others",
            "--exclude-standard",
            "-z",
            "--",
            &pathspec,
        ],
    )?;

    let mut files: Vec<PathBuf> = split_paths(dir, &output)
        .into_iter()
        .filter(|f| f.exists())
        .collect();
    files.sort();
    files.dedup();
    Ok(files)
}

/// Découpe une sortie `-z` de git en chemins absolus
fn split_paths(root: &Path, output: &str) -> Vec<PathBuf> {
    output
        .split(['\0', '\n'])
        .filter(|f| !f.is_empty())
        .map(|f| root.join(f))
        .collect()
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

    #[test]
    fn list_pushed_files() {
        let dir = tempdir().unwrap();
        let remote = dir.path().join("remote.git");
        let root = dir.path().join("repo");
        fs::create_dir_all(&root).unwrap();
        let root = root.canonicalize().unwrap();

        git(dir.path(), &["init", "-q", "--bare", "remote.git"]).unwrap();
        init_repo(&root);
        git(
            &root,
            &["remote", "add", "origin", &remote.to_string_lossy()],
        )
        .unwrap();

        fs::write(root.join("old.rs"), "").unwrap();
        git(&root, &["add", "."]).unwrap();
        git(&root, &["commit", "-q", "-m", "init"]).unwrap();
        git(&root, &["push", "-q", "origin", "HEAD:refs/heads/main"]).unwrap();
        let remote_sha = git(&root, &["rev-parse", "HEAD"]).unwrap();

        fs::create_dir_all(root.join("api")).unwrap();
        fs::write(root.join("api/new.rs"), "").unwrap();
        git(&root, &["add", "."]).unwrap();
        git(&root, &["commit", "-q", "-m", "api"]).unwrap();
        let local_sha = git(&root, &["rev-parse", "HEAD"]).unwrap();

        let refs = format!(
            "refs/heads/main {} refs/heads/main {}\n",
            local_sha.trim(),
            remote_sha.trim()
        );
        assert_eq!(
            pushed_files(&root, "origin", &refs).unwrap(),
            vec![root.join("api/new.rs")]
        );

        // suppression de branche → rien
        let refs = format!(
            "(delete) {} refs/heads/old {}\n",
            "0".repeat(40),
            remote_sha.trim()
        );
        assert!(pushed_files(&root, "origin", &refs).unwrap().is_empty());
    }

    #[test]
    fn find_config_files() {
        let dir = tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        init_repo(&root);

        fs::create_dir_all(root.join("a/b")).unwrap();
        fs::create_dir_all(root.join("ignored")).unwrap();
        fs::write(root.join(".gitignore"), "ignored/\n").unwrap();
        for f in ["c.json", "a/c.json", "a/b/c.json", "ignored/c.json"] {
            fs::write(root.join(f), "").unwrap();
        }

        assert_eq!(
            find_files(&root, "c.json").unwrap(),
            vec![
                root.join("a/b/c.json"),
                root.join("a/c.json"),
                root.join("c.json")
            ]
        );
    }
}
