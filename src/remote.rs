use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use crate::errors::GoatHookerError;

/// Dossier de cache des repos `from`
/// (`$HOOKER_CACHE_DIR`, sinon `$XDG_CACHE_HOME/goat-hooker`, sinon `~/.cache/goat-hooker`)
pub fn cache_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("HOOKER_CACHE_DIR") {
        return PathBuf::from(dir);
    }
    if let Some(dir) = std::env::var_os("XDG_CACHE_HOME") {
        return PathBuf::from(dir).join("goat-hooker");
    }
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .unwrap_or_else(|| ".".into());
    PathBuf::from(home).join(".cache").join("goat-hooker")
}

/// Dossier du repo en cache pour une URL + rev
pub fn repo_dir(url: &str, rev: Option<&str>) -> PathBuf {
    let key = format!("{url}#{}", rev.unwrap_or_default());
    cache_dir()
        .join("repos")
        .join(format!("{:016x}", fnv1a(&key)))
}

/// Retourne le repo en cache, en le clonant s'il n'y est pas encore.
/// Pas de réseau si le repo est déjà là : `hooker update` le met à jour.
pub fn fetch(url: &str, rev: Option<&str>) -> Result<PathBuf, GoatHookerError> {
    let dir = repo_dir(url, rev);
    if dir.join(".git").exists() {
        return Ok(dir);
    }

    let parent = dir.parent().unwrap();
    fs::create_dir_all(parent)?;

    // Clone dans un dossier temporaire puis renomme : pas de repo à moitié cloné en cache
    let tmp = parent.join(format!(
        "{}.tmp-{}",
        dir.file_name().unwrap().to_string_lossy(),
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&tmp);

    // init + fetch instead of clone: `rev` can be a branch, a tag or a commit
    eprintln!("📥 Fetching hooks from {url}...");
    fs::create_dir_all(&tmp)?;
    let fetched = git(&tmp, &["init", "--quiet"])
        .and_then(|_| git(&tmp, &["remote", "add", "origin", url]))
        .and_then(|_| {
            git(
                &tmp,
                &[
                    "fetch",
                    "--quiet",
                    "--depth",
                    "1",
                    "origin",
                    rev.unwrap_or("HEAD"),
                ],
            )
        })
        .and_then(|_| git(&tmp, &["checkout", "--quiet", "FETCH_HEAD"]));
    if let Err(e) = fetched {
        let _ = fs::remove_dir_all(&tmp);
        return Err(GoatHookerError::GitError(format!(
            "failed to clone {url}: {e}"
        )));
    }

    if fs::rename(&tmp, &dir).is_err() {
        // un autre process l'a cloné en même temps
        let _ = fs::remove_dir_all(&tmp);
    }
    Ok(dir)
}

/// Met à jour un repo en cache sur la dernière version de `rev`
pub fn update(repo: &Path, rev: Option<&str>) -> Result<(), GoatHookerError> {
    let target = rev.unwrap_or("HEAD");
    git(
        repo,
        &["fetch", "--quiet", "--depth", "1", "origin", target],
    )?;
    git(repo, &["reset", "--quiet", "--hard", "FETCH_HEAD"])?;
    Ok(())
}

/// Commit of the repository in cache
pub fn head(repo: &Path) -> Option<String> {
    git_output(repo, &["rev-parse", "HEAD"])
}

/// Last commit of `rev` (or of the default branch) on the remote. `None` for a commit:
/// it can't change
pub fn latest(url: &str, rev: Option<&str>) -> Result<Option<String>, GoatHookerError> {
    if rev.is_some_and(is_commit) {
        return Ok(None);
    }
    let dir = std::env::temp_dir();
    let refs = git_output(&dir, &["ls-remote", url, rev.unwrap_or("HEAD")])
        .ok_or_else(|| GoatHookerError::GitError(format!("can't reach {url}")))?;
    // A tag is listed twice (`v1` and its commit `v1^{}`): the commit comes last
    Ok(refs
        .lines()
        .filter_map(|l| l.split_whitespace().next())
        .next_back()
        .map(str::to_string))
}

/// A full commit id
pub fn is_commit(rev: &str) -> bool {
    rev.len() == 40 && rev.chars().all(|c| c.is_ascii_hexdigit())
}

fn git_output(dir: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn git(dir: &Path, args: &[&str]) -> Result<(), GoatHookerError> {
    let output = Command::new("git")
        .args(args)
        .current_dir(dir)
        // pas de prompt de mot de passe dans un hook
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .map_err(|e| GoatHookerError::GitError(format!("failed to run git: {e}")))?;

    if !output.status.success() {
        return Err(GoatHookerError::GitError(
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ));
    }
    Ok(())
}

/// Hash stable entre les versions de Rust (contrairement à `DefaultHasher`)
fn fnv1a(s: &str) -> u64 {
    s.bytes().fold(0xcbf29ce484222325, |hash, byte| {
        (hash ^ byte as u64).wrapping_mul(0x100000001b3)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repo_dir_depends_on_url_and_rev() {
        let a = repo_dir("https://x/a.git", None);
        assert_eq!(a, repo_dir("https://x/a.git", None));
        assert_ne!(a, repo_dir("https://x/a.git", Some("v1")));
        assert_ne!(a, repo_dir("https://x/b.git", None));
    }
}
