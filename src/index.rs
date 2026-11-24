//! Reads the Git index to tell, without starting git, that the work tree has no unstaged
//! change: the files have the size, modification time and type the index recorded.
//! It is the check `git diff` starts with. When anything is unusual (racy timestamps, split
//! or sparse index, conflicts, another hash), the answer is "don't know" and git is asked.

use std::{
    fs,
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

/// Above this many files, git (which also reads them faster with its own caches) is asked
const MAX_ENTRIES: usize = 100_000;

/// One file of the index, the fields compared with the work tree
#[derive(Debug, PartialEq)]
struct Entry {
    path: Vec<u8>,
    mtime: (u32, u32),
    mode: u32,
    size: u32,
    /// Conflict stage (0 when there is none)
    stage: u16,
    /// Not checked by git: `assume-unchanged` or `skip-worktree`
    unchecked: bool,
    /// `git add -N`: shown as changed by `git diff`
    intent_to_add: bool,
}

/// Index file read by git for this work tree (`GIT_INDEX_FILE` while committing)
fn index_path(root: &Path, git_dir: &Path) -> PathBuf {
    match std::env::var_os("GIT_INDEX_FILE") {
        Some(file) => {
            let file = PathBuf::from(file);
            if file.is_absolute() {
                file
            } else {
                root.join(file)
            }
        }
        None => git_dir.join("index"),
    }
}

/// `true` when the work tree surely has no unstaged change, `false` when git must be asked
pub fn worktree_is_clean(root: &Path, git_dir: &Path) -> bool {
    check(root, git_dir).unwrap_or(false)
}

fn check(root: &Path, git_dir: &Path) -> Option<bool> {
    let config = fs::read_to_string(git_dir.join("config")).unwrap_or_default();
    let config = config.to_ascii_lowercase().replace([' ', '\t'], "");
    // sha256 repositories, or settings changing what git compares
    if config.contains("objectformat") || config.contains("filemode=false") {
        return None;
    }
    let path = index_path(root, git_dir);
    let data = fs::read(&path).ok()?;
    let index_time = fs::metadata(&path).ok()?.modified().ok()?;
    let index_time = index_time.duration_since(UNIX_EPOCH).ok()?;
    let index_time = (index_time.as_secs() as u32, index_time.subsec_nanos());

    let entries = parse(&data)?;
    let unchanged = |entry: &Entry| -> bool {
        if entry.unchecked || entry.mode == 0o160000 {
            return true;
        }
        if entry.stage != 0 || entry.intent_to_add {
            return false;
        }
        // Written in the same instant as the index: the content may have changed since
        if entry.mtime >= index_time {
            return false;
        }
        let Some(path) = path_of(&entry.path) else {
            return false;
        };
        fs::symlink_metadata(root.join(path)).is_ok_and(|meta| same_file(entry, &meta))
    };
    // Big work trees: the files are checked by several threads, like git does
    let threads = std::thread::available_parallelism().map_or(1, |n| n.get().min(8));
    if entries.len() < 1000 || threads == 1 {
        return Some(entries.iter().all(unchanged));
    }
    let chunk = entries.len().div_ceil(threads);
    let clean = std::thread::scope(|scope| {
        let workers: Vec<_> = entries
            .chunks(chunk)
            .map(|part| scope.spawn(|| part.iter().all(unchanged)))
            .collect();
        workers.into_iter().all(|w| w.join().unwrap_or(false))
    });
    Some(clean)
}

#[cfg(unix)]
fn path_of(bytes: &[u8]) -> Option<PathBuf> {
    use std::os::unix::ffi::OsStrExt;
    Some(PathBuf::from(std::ffi::OsStr::from_bytes(bytes)))
}

#[cfg(not(unix))]
fn path_of(bytes: &[u8]) -> Option<PathBuf> {
    std::str::from_utf8(bytes).ok().map(PathBuf::from)
}

fn same_file(entry: &Entry, meta: &fs::Metadata) -> bool {
    let kind = entry.mode & 0o170000;
    let right_kind = match kind {
        0o100000 => meta.is_file(),
        0o120000 => meta.file_type().is_symlink(),
        _ => false,
    };
    if !right_kind || meta.len() as u32 != entry.size {
        return false;
    }
    let Some(modified) = meta
        .modified()
        .ok()
        .and_then(|m| m.duration_since(UNIX_EPOCH).ok())
    else {
        return false;
    };
    if modified.as_secs() as u32 != entry.mtime.0 {
        return false;
    }
    // Git built without nanoseconds stores 0
    if entry.mtime.1 != 0 && modified.subsec_nanos() != entry.mtime.1 {
        return false;
    }
    #[cfg(unix)]
    if kind == 0o100000 {
        use std::os::unix::fs::PermissionsExt;
        let executable = meta.permissions().mode() & 0o100 != 0;
        if executable != (entry.mode & 0o100 != 0) {
            return false;
        }
    }
    true
}

fn be32(data: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(data.get(at..at + 4)?.try_into().ok()?))
}

fn be16(data: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_be_bytes(data.get(at..at + 2)?.try_into().ok()?))
}

/// Entries of an index (versions 2 to 4), `None` for what isn't supported
fn parse(data: &[u8]) -> Option<Vec<Entry>> {
    if data.get(..4)? != b"DIRC" {
        return None;
    }
    let version = be32(data, 4)?;
    if !(2..=4).contains(&version) {
        return None;
    }
    let count = be32(data, 8)? as usize;
    if count > MAX_ENTRIES {
        return None;
    }
    let mut entries = Vec::with_capacity(count);
    let mut at = 12;
    let mut previous: Vec<u8> = Vec::new();
    for _ in 0..count {
        let start = at;
        let mtime = (be32(data, at + 8)?, be32(data, at + 12)?);
        let mode = be32(data, at + 24)?;
        let size = be32(data, at + 36)?;
        let flags = be16(data, at + 60)?;
        at += 62;
        let mut extended = 0;
        if flags & 0x4000 != 0 {
            if version < 3 {
                return None;
            }
            extended = be16(data, at)?;
            at += 2;
        }
        let path = if version == 4 {
            // Prefix-compressed: how many bytes of the previous path to drop, then the rest
            let (strip, used) = varint(data.get(at..)?)?;
            at += used;
            let end = at + data.get(at..)?.iter().position(|&b| b == 0)?;
            let keep = previous.len().checked_sub(strip)?;
            let mut path = previous[..keep].to_vec();
            path.extend_from_slice(&data[at..end]);
            at = end + 1;
            path
        } else {
            let end = at + data.get(at..)?.iter().position(|&b| b == 0)?;
            let path = data[at..end].to_vec();
            // Padded with 1 to 8 NULs to a multiple of 8
            at = start + ((end - start + 8) & !7);
            path
        };
        // Sparse index: a whole folder as one entry
        if mode & 0o170000 == 0o040000 {
            return None;
        }
        previous = path.clone();
        entries.push(Entry {
            path,
            mtime,
            mode,
            size,
            stage: (flags >> 12) & 3,
            unchecked: flags & 0x8000 != 0 || extended & 0x4000 != 0,
            intent_to_add: extended & 0x2000 != 0,
        });
    }
    // Extensions: a split index keeps entries in another file
    while at + 8 <= data.len().saturating_sub(20) {
        let signature = data.get(at..at + 4)?;
        if signature == b"link" || signature == b"sdir" {
            return None;
        }
        at += 8 + be32(data, at + 4)? as usize;
    }
    Some(entries)
}

/// Variable-length integer of the index version 4 (`(n + 1) << 7 | next` encoding)
fn varint(data: &[u8]) -> Option<(usize, usize)> {
    let mut used = 0;
    let mut byte = *data.get(used)?;
    used += 1;
    let mut value = (byte & 0x7f) as usize;
    while byte & 0x80 != 0 {
        byte = *data.get(used)?;
        used += 1;
        value = ((value + 1) << 7) | (byte & 0x7f) as usize;
    }
    Some((value, used))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;
    use tempfile::tempdir;

    fn git(dir: &Path, args: &[&str]) {
        let ok = Command::new("git")
            .args(args)
            .current_dir(dir)
            .env("HOOKER_SKIP", "1")
            .status()
            .unwrap()
            .success();
        assert!(ok, "git {args:?}");
    }

    fn repo(version: &str) -> tempfile::TempDir {
        let dir = tempdir().unwrap();
        git(dir.path(), &["init", "-q"]);
        git(dir.path(), &["config", "user.name", "t"]);
        git(dir.path(), &["config", "user.email", "t@t"]);
        git(dir.path(), &["config", "core.autocrlf", "false"]);
        git(dir.path(), &["config", "index.version", version]);
        fs::create_dir_all(dir.path().join("src/deep")).unwrap();
        for (file, content) in [
            ("a.txt", "a"),
            ("src/b.rs", "b"),
            ("src/deep/c.rs", "c"),
            ("src/deep/cd.rs", "cd"),
        ] {
            fs::write(dir.path().join(file), content).unwrap();
        }
        git(dir.path(), &["add", "."]);
        git(dir.path(), &["commit", "-qm", "init", "--no-verify"]);
        // Older than the index: not racy
        std::thread::sleep(std::time::Duration::from_millis(20));
        git(dir.path(), &["update-index", "--refresh", "-q"]);
        dir
    }

    fn clean(dir: &Path) -> bool {
        worktree_is_clean(dir, &dir.join(".git"))
    }

    #[test]
    fn reads_every_index_version() {
        for version in ["2", "3", "4"] {
            let dir = repo(version);
            let data = fs::read(dir.path().join(".git/index")).unwrap();
            let paths: Vec<String> = parse(&data)
                .unwrap()
                .into_iter()
                .map(|e| String::from_utf8(e.path).unwrap())
                .collect();
            assert_eq!(
                paths,
                ["a.txt", "src/b.rs", "src/deep/c.rs", "src/deep/cd.rs"],
                "version {version}"
            );
            assert!(clean(dir.path()), "version {version}");
        }
    }

    #[test]
    fn sees_changes() {
        let dir = repo("2");
        fs::write(dir.path().join("src/b.rs"), "changed").unwrap();
        assert!(!clean(dir.path()));

        let dir = repo("2");
        fs::remove_file(dir.path().join("a.txt")).unwrap();
        assert!(!clean(dir.path()));

        let dir = repo("4");
        fs::write(dir.path().join("new.txt"), "x").unwrap();
        git(dir.path(), &["add", "-N", "new.txt"]);
        assert!(!clean(dir.path()));
    }

    #[test]
    fn untracked_files_dont_count() {
        let dir = repo("2");
        fs::write(dir.path().join("untracked.txt"), "x").unwrap();
        assert!(clean(dir.path()));
    }

    #[test]
    fn varints() {
        assert_eq!(varint(&[0x05]), Some((5, 1)));
        // 128 = ((0 + 1) << 7) | 0
        assert_eq!(varint(&[0x80, 0x00]), Some((128, 2)));
    }
}
