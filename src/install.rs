use std::{
    fs::{self, create_dir_all},
    path::{Path, PathBuf},
};

use crate::{config::Trigger, errors::GoatHookerError, git};

/// Marks the hooks installed by goat-hooker (not to overwrite the ones of the user)
pub const HOOK_MARKER: &str = "# installed by goat-hooker";

/// Installs the hooks of the repository.
/// Returns the hand-written hooks that were kept (unless `force`).
pub fn install_hooks(root: &Path, force: bool) -> Result<Vec<PathBuf>, GoatHookerError> {
    install_hooks_in(&git::hooks_dir(root)?, force)
}

/// All the hooks are installed by goat-hooker
pub fn is_installed(root: &Path) -> bool {
    let Ok(hooks_dir) = git::hooks_dir(root) else {
        return false;
    };
    Trigger::ALL.iter().all(|trigger| {
        fs::read_to_string(hooks_dir.join(trigger.as_str()))
            .is_ok_and(|content| content.contains(HOOK_MARKER))
    })
}

fn install_hooks_in(hooks_dir: &Path, force: bool) -> Result<Vec<PathBuf>, GoatHookerError> {
    // Creates the hooks folder if needed
    create_dir_all(hooks_dir)?;
    let mut skipped = Vec::new();

    for trigger in Trigger::ALL {
        let hook_name = trigger.as_str();
        let hook_path = hooks_dir.join(hook_name);

        // Don't overwrite a hand-written hook
        if hook_path.exists() && !force {
            let content = fs::read_to_string(&hook_path).unwrap_or_default();
            if !content.contains(HOOK_MARKER) {
                skipped.push(hook_path);
                continue;
            }
        }

        let script = format!(
            "#!/bin/sh\n{HOOK_MARKER}\n[ \"$HOOKER_SKIP\" = 1 ] && exit 0\nexec hooker run {hook_name} --from-git -- \"$@\"\n"
        );
        fs::write(&hook_path, script)?;

        // Make the file executable (chmod 755)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&hook_path, fs::Permissions::from_mode(0o755))?;
        }
    }

    Ok(skipped)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn install_all_hooks() {
        let dir = tempdir().unwrap();
        install_hooks_in(dir.path(), false).unwrap();

        for trigger in Trigger::ALL {
            let content = fs::read_to_string(dir.path().join(trigger.as_str())).unwrap();
            assert!(content.contains(&format!("hooker run {}", trigger.as_str())));
        }
    }

    #[test]
    fn keep_user_hooks_unless_forced() {
        let dir = tempdir().unwrap();
        let pre_commit = dir.path().join("pre-commit");
        fs::write(&pre_commit, "#!/bin/sh\necho mine\n").unwrap();

        let skipped = install_hooks_in(dir.path(), false).unwrap();
        assert_eq!(skipped, vec![pre_commit.clone()]);
        assert!(
            fs::read_to_string(&pre_commit)
                .unwrap()
                .contains("echo mine")
        );

        install_hooks_in(dir.path(), true).unwrap();
        assert!(
            fs::read_to_string(&pre_commit)
                .unwrap()
                .contains(HOOK_MARKER)
        );
    }

    #[test]
    fn detect_installed_hooks() {
        let dir = tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        std::process::Command::new("git")
            .args(["init", "-q"])
            .current_dir(&root)
            .status()
            .unwrap();

        assert!(!is_installed(&root));
        install_hooks(&root, false).unwrap();
        assert!(is_installed(&root));
    }
}
