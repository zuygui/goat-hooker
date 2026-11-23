use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use crate::errors::GoatHookerError;

/// Variable d'env qui désactive goat-hooker (`HOOKER_SKIP=1 git commit`).
/// Aussi utilisée pour nos propres commandes git (le `checkout` du stash ne doit pas lancer le post-checkout).
pub const SKIP_ENV: &str = "HOOKER_SKIP";

/// Lance une commande git dans `dir` et retourne sa sortie standard
fn git(dir: &Path, args: &[&str]) -> Result<String, GoatHookerError> {
    git_bytes(dir, args).map(|out| String::from_utf8_lossy(&out).to_string())
}

/// Comme `git`, mais garde la sortie brute (les patchs ne sont pas forcément en UTF-8)
fn git_bytes(dir: &Path, args: &[&str]) -> Result<Vec<u8>, GoatHookerError> {
    let output = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env(SKIP_ENV, "1")
        .output()
        .map_err(|e| GoatHookerError::GitError(format!("failed to run git: {e}")))?;

    if !output.status.success() {
        return Err(GoatHookerError::GitError(
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ));
    }

    Ok(output.stdout)
}

/// Chemin d'un fichier dans le dossier `.git` (gère les worktrees)
fn git_path(root: &Path, name: &str) -> Result<PathBuf, GoatHookerError> {
    let path = PathBuf::from(git(root, &["rev-parse", "--git-path", name])?.trim());
    Ok(if path.is_absolute() {
        path
    } else {
        root.join(path)
    })
}

/// Ajoute des fichiers à l'index (pour `stage_fixed`)
pub fn add(root: &Path, files: &[PathBuf]) -> Result<(), GoatHookerError> {
    if files.is_empty() {
        return Ok(());
    }
    let files: Vec<String> = files
        .iter()
        .map(|f| f.to_string_lossy().to_string())
        .collect();
    let mut args = vec!["add", "--"];
    args.extend(files.iter().map(String::as_str));
    git(root, &args)?;
    Ok(())
}

/// Un fichier avec des modifs non stagées
struct StashedFile {
    path: PathBuf,
    /// Blob de la version stagée (avant les hooks)
    base: String,
    /// Blob de la version du working tree (`None` si le fichier a été supprimé)
    saved: Option<String>,
}

/// Modifs non stagées mises de côté pendant le pre-commit,
/// pour que les hooks voient exactement ce qui va être commité.
///
/// Les versions du working tree sont gardées dans la base d'objets de git,
/// et la liste dans `.git/goat-hooker-<pid>.stash` (pour les retrouver en cas de crash).
pub struct UnstagedStash {
    root: PathBuf,
    manifest: PathBuf,
    files: Vec<StashedFile>,
    restored: bool,
}

impl UnstagedStash {
    /// Met de côté les modifs non stagées des fichiers suivis (rien à faire s'il n'y en a pas)
    pub fn save(root: &Path) -> Result<Option<Self>, GoatHookerError> {
        // Pendant un merge, l'index contient des conflits : on ne touche à rien
        if git(root, &["rev-parse", "-q", "--verify", "MERGE_HEAD"]).is_ok() {
            return Ok(None);
        }

        let output = git(
            root,
            &[
                "diff",
                "--name-only",
                "--no-renames",
                "--ignore-submodules",
                "-z",
            ],
        )?;
        let paths: Vec<&str> = output.split('\0').filter(|f| !f.is_empty()).collect();
        if paths.is_empty() {
            return Ok(None);
        }

        let mut files = Vec::new();
        for path in &paths {
            let base = git(root, &["rev-parse", &format!(":{path}")])?
                .trim()
                .to_string();
            let saved = if root.join(path).exists() {
                Some(
                    git(root, &["hash-object", "-w", "--", path])?
                        .trim()
                        .to_string(),
                )
            } else {
                None
            };
            files.push(StashedFile {
                path: PathBuf::from(path),
                base,
                saved,
            });
        }

        let manifest = git_path(root, &format!("goat-hooker-{}.stash", std::process::id()))?;
        let lines: Vec<String> = files
            .iter()
            .map(|f| {
                format!(
                    "{} {} {}",
                    f.base,
                    f.saved.as_deref().unwrap_or("-"),
                    f.path.display()
                )
            })
            .collect();
        fs::write(&manifest, lines.join("\n") + "\n")?;
        // From here, dropping it (error, panic) puts the changes back
        let stash = Self {
            root: root.to_path_buf(),
            manifest,
            files,
            restored: false,
        };

        // Remet ces fichiers au niveau de l'index, sans lancer les hooks de Git (post-checkout)
        let no_hooks = format!(
            "core.hooksPath={}",
            git_path(root, "goat-hooker/no-hooks")?.display()
        );
        let mut args = vec!["-c", no_hooks.as_str(), "checkout", "--"];
        args.extend(paths.iter().copied());
        git(root, &args)?;

        Ok(Some(stash))
    }

    /// Remet les modifs non stagées.
    /// Si les hooks ont modifié un fichier (formateur), leurs modifs sont fusionnées avec les tiennes ;
    /// en cas de conflit, le working tree garde ta version (les modifs des hooks restent dans l'index).
    pub fn restore(&mut self) -> Result<(), GoatHookerError> {
        if self.restored {
            return Ok(());
        }
        self.restored = true;

        for file in &self.files {
            let path = self.root.join(&file.path);
            let base = git_bytes(&self.root, &["cat-file", "blob", &file.base])?;
            let current = fs::read(&path).ok();

            let Some(saved) = file.saved.as_ref() else {
                // supprimé dans le working tree
                if current.as_deref() == Some(base.as_slice()) {
                    fs::remove_file(&path)?;
                }
                continue;
            };
            let saved = git_bytes(&self.root, &["cat-file", "blob", saved])?;

            let restored = match current {
                // les hooks n'ont pas touché au fichier
                Some(current) if current == base => saved,
                Some(current) => match merge_file(&self.root, &current, &base, &saved)? {
                    Some(merged) => merged,
                    None => {
                        eprintln!(
                            "⚠️ Warning: hooks changes on {} conflict with your unstaged changes. They are staged, your working copy keeps your version.",
                            file.path.display()
                        );
                        saved
                    }
                },
                None => saved,
            };
            fs::write(&path, restored)?;
        }

        fs::remove_file(&self.manifest)?;
        Ok(())
    }
}

impl Drop for UnstagedStash {
    // Filet de sécurité si `restore` n'a pas été appelé (erreur, panic...)
    fn drop(&mut self) {
        if let Err(err) = self.restore() {
            eprintln!(
                "❌ {err} (unstaged changes are listed in {})",
                self.manifest.display()
            );
        }
    }
}

/// Fusion 3-way avec `git merge-file` (`None` en cas de conflit)
fn merge_file(
    root: &Path,
    current: &[u8],
    base: &[u8],
    other: &[u8],
) -> Result<Option<Vec<u8>>, GoatHookerError> {
    let dir = git_path(root, &format!("goat-hooker-merge-{}", std::process::id()))?;
    fs::create_dir_all(&dir)?;
    let (current_file, base_file, other_file) =
        (dir.join("current"), dir.join("base"), dir.join("other"));
    fs::write(&current_file, current)?;
    fs::write(&base_file, base)?;
    fs::write(&other_file, other)?;

    let output = Command::new("git")
        .arg("merge-file")
        .arg("-p")
        .args([&current_file, &base_file, &other_file])
        .current_dir(root)
        .output();
    let _ = fs::remove_dir_all(&dir);

    let output =
        output.map_err(|e| GoatHookerError::GitError(format!("failed to run git: {e}")))?;
    // 0 = fusion propre, > 0 = nombre de conflits, < 0 = erreur (ex: fichier binaire)
    Ok(output.status.success().then_some(output.stdout))
}

/// Racine du dépôt
pub fn repo_root(dir: &Path) -> Result<PathBuf, GoatHookerError> {
    let root = git(dir, &["rev-parse", "--show-toplevel"])?;
    Ok(PathBuf::from(root.trim()))
}

/// Branche courante (`None` en HEAD détaché)
pub fn current_branch(dir: &Path) -> Option<String> {
    let branch = git(dir, &["symbolic-ref", "--quiet", "--short", "HEAD"]).ok()?;
    Some(branch.trim().to_string()).filter(|b| !b.is_empty())
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

/// Folder of goat-hooker's own data in `.git` (variables, trusted hooks)
pub fn data_dir(root: &Path) -> Result<PathBuf, GoatHookerError> {
    git_path(root, "goat-hooker")
}

/// `core.hooksPath` when set (husky and other managers set it)
pub fn hooks_path_setting(root: &Path) -> Option<String> {
    git(root, &["config", "--get", "core.hooksPath"])
        .ok()
        .map(|p| p.trim().to_string())
        .filter(|p| !p.is_empty())
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

    #[test]
    fn stash_doesnt_run_git_hooks() {
        let dir = tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        init_repo(&root);
        fs::write(root.join("a.rs"), "v1\n").unwrap();
        git(&root, &["add", "."]).unwrap();
        git(&root, &["commit", "-q", "-m", "init"]).unwrap();

        // a post-checkout hook that fails (like a hooker missing from the PATH)
        let hook = root.join(".git/hooks/post-checkout");
        fs::write(&hook, "#!/bin/sh\nexit 1\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&hook, fs::Permissions::from_mode(0o755)).unwrap();
        }

        fs::write(root.join("a.rs"), "v2\n").unwrap();
        let mut stash = UnstagedStash::save(&root).unwrap().unwrap();
        assert_eq!(fs::read_to_string(root.join("a.rs")).unwrap(), "v1\n");
        stash.restore().unwrap();
        assert_eq!(fs::read_to_string(root.join("a.rs")).unwrap(), "v2\n");
    }

    #[test]
    fn stash_keeps_only_staged_changes() {
        let dir = tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        init_repo(&root);

        fs::write(root.join("a.rs"), "v1\n").unwrap();
        git(&root, &["add", "."]).unwrap();
        git(&root, &["commit", "-q", "-m", "init"]).unwrap();

        // v2 stagé, v3 non stagé
        fs::write(root.join("a.rs"), "v2\n").unwrap();
        git(&root, &["add", "a.rs"]).unwrap();
        fs::write(root.join("a.rs"), "v3\n").unwrap();

        let mut stash = UnstagedStash::save(&root).unwrap().unwrap();
        // les hooks voient la version stagée
        assert_eq!(fs::read_to_string(root.join("a.rs")).unwrap(), "v2\n");

        stash.restore().unwrap();
        assert_eq!(fs::read_to_string(root.join("a.rs")).unwrap(), "v3\n");
        assert_eq!(staged_files(&root).unwrap(), vec![root.join("a.rs")]);
    }

    #[test]
    fn stash_merges_hook_changes() {
        let dir = tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        init_repo(&root);

        fs::write(root.join("a.rs"), "a\nb\nc\nd\ne\n").unwrap();
        git(&root, &["add", "."]).unwrap();
        git(&root, &["commit", "-q", "-m", "init"]).unwrap();
        // modif non stagée en fin de fichier
        fs::write(root.join("a.rs"), "a\nb\nc\nd\ne\nunstaged\n").unwrap();

        let mut stash = UnstagedStash::save(&root).unwrap().unwrap();
        // un formateur modifie le début du fichier et re-stage
        fs::write(root.join("a.rs"), "A\nb\nc\nd\ne\n").unwrap();
        add(&root, &[root.join("a.rs")]).unwrap();
        stash.restore().unwrap();

        // les deux sont gardées dans le working tree, seule la correction est stagée
        assert_eq!(
            fs::read_to_string(root.join("a.rs")).unwrap(),
            "A\nb\nc\nd\ne\nunstaged\n"
        );
        let staged = git(&root, &["show", ":a.rs"]).unwrap();
        assert_eq!(staged, "A\nb\nc\nd\ne\n");
    }

    #[test]
    fn stash_keeps_user_version_on_conflict() {
        let dir = tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        init_repo(&root);

        fs::write(root.join("a.rs"), "hello\n").unwrap();
        git(&root, &["add", "."]).unwrap();
        git(&root, &["commit", "-q", "-m", "init"]).unwrap();
        fs::write(root.join("a.rs"), "hello\nunstaged\n").unwrap();

        {
            let _stash = UnstagedStash::save(&root).unwrap().unwrap();
            // un formateur modifie la même zone → conflit
            fs::write(root.join("a.rs"), "HELLO\n").unwrap();
            add(&root, &[root.join("a.rs")]).unwrap();
            // restauré par le Drop
        }
        assert_eq!(
            fs::read_to_string(root.join("a.rs")).unwrap(),
            "hello\nunstaged\n"
        );
        assert_eq!(git(&root, &["show", ":a.rs"]).unwrap(), "HELLO\n");
    }

    #[test]
    fn stash_unstaged_deletion() {
        let dir = tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        init_repo(&root);

        fs::write(root.join("a.rs"), "a\n").unwrap();
        git(&root, &["add", "."]).unwrap();
        git(&root, &["commit", "-q", "-m", "init"]).unwrap();
        fs::remove_file(root.join("a.rs")).unwrap();

        let mut stash = UnstagedStash::save(&root).unwrap().unwrap();
        assert!(root.join("a.rs").exists());
        stash.restore().unwrap();
        assert!(!root.join("a.rs").exists());
    }

    #[test]
    fn nothing_to_stash() {
        let dir = tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        init_repo(&root);
        fs::write(root.join("a.rs"), "").unwrap();
        git(&root, &["add", "."]).unwrap();

        assert!(UnstagedStash::save(&root).unwrap().is_none());
    }
}
