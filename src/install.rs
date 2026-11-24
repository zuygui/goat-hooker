use std::{
    fs::{self, create_dir_all},
    path::{Path, PathBuf},
};

use crate::{
    config::{Config, ConfigLoader, Trigger, config_path},
    errors::GoatHookerError,
    git,
};

/// Marks the hooks installed by goat-hooker (not to overwrite the ones of the user)
pub const HOOK_MARKER: &str = "# installed by goat-hooker";

/// Triggers whose Git hook is installed: the ones the configs use, and pre-commit, which
/// runs on every commit and installs the others when the configs change (see `sync`)
pub fn used_triggers(configs: &[Config]) -> Vec<Trigger> {
    Trigger::ALL
        .iter()
        .copied()
        .filter(|trigger| {
            *trigger == Trigger::PreCommit
                || configs
                    .iter()
                    .flat_map(|c| c.hooks.iter().flatten())
                    .any(|h| h.trigger == Some(*trigger))
        })
        .collect()
}

/// Triggers to install for the repository (every trigger when its config can't be read)
fn triggers_of(root: &Path) -> Vec<Trigger> {
    ConfigLoader::new()
        .load_recursive(&config_path(root))
        .map(|configs| used_triggers(&configs))
        .unwrap_or_else(|_| Trigger::ALL.to_vec())
}

/// Installs the Git hooks of the triggers the repository uses.
/// Returns the hand-written hooks that were kept (unless `force`).
pub fn install_hooks(root: &Path, force: bool) -> Result<Vec<PathBuf>, GoatHookerError> {
    install_hooks_in(&git::hooks_dir(root)?, &triggers_of(root), force)
}

/// The Git hooks of every trigger the repository uses are installed by goat-hooker
pub fn is_installed(root: &Path) -> bool {
    let Ok(hooks_dir) = git::hooks_dir(root) else {
        return false;
    };
    triggers_of(root)
        .iter()
        .all(|trigger| ours(&hooks_dir.join(trigger.as_str())))
}

/// Keeps the installed Git hooks in line with the configs, when Git runs one of them:
/// adds the triggers a config started to use, removes ours that aren't used anymore.
/// Hand-written hooks are left alone.
pub fn sync(root: &Path, configs: &[Config]) -> Result<(), GoatHookerError> {
    let hooks_dir = git::hooks_dir(root)?;
    let used = used_triggers(configs);
    for trigger in Trigger::ALL {
        let path = hooks_dir.join(trigger.as_str());
        if used.contains(&trigger) {
            if !path.exists() {
                write_hook(&path, trigger)?;
            }
        } else if ours(&path) {
            fs::remove_file(&path)?;
        }
    }
    Ok(())
}

fn ours(path: &Path) -> bool {
    fs::read_to_string(path).is_ok_and(|content| content.contains(HOOK_MARKER))
}

fn write_hook(path: &Path, trigger: Trigger) -> Result<(), GoatHookerError> {
    let hook_name = trigger.as_str();
    let script = format!(
        "#!/bin/sh\n{HOOK_MARKER}\n[ \"$HOOKER_SKIP\" = 1 ] && exit 0\nexec hooker run {hook_name} --from-git -- \"$@\"\n"
    );
    fs::write(path, script)?;

    // Make the file executable (chmod 755)
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o755))?;
    }
    Ok(())
}

fn install_hooks_in(
    hooks_dir: &Path,
    triggers: &[Trigger],
    force: bool,
) -> Result<Vec<PathBuf>, GoatHookerError> {
    // Creates the hooks folder if needed
    create_dir_all(hooks_dir)?;
    let mut skipped = Vec::new();

    for trigger in Trigger::ALL {
        let hook_path = hooks_dir.join(trigger.as_str());
        if !triggers.contains(&trigger) {
            // Installed by an older version, or the trigger isn't used anymore
            if ours(&hook_path) {
                fs::remove_file(&hook_path)?;
            }
            continue;
        }

        // Don't overwrite a hand-written hook
        if hook_path.exists() && !force && !ours(&hook_path) {
            skipped.push(hook_path);
            continue;
        }
        write_hook(&hook_path, trigger)?;
    }

    Ok(skipped)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn install_the_used_hooks() {
        let dir = tempdir().unwrap();
        install_hooks_in(dir.path(), &Trigger::ALL, false).unwrap();
        for trigger in Trigger::ALL {
            let content = fs::read_to_string(dir.path().join(trigger.as_str())).unwrap();
            assert!(content.contains(&format!("hooker run {}", trigger.as_str())));
        }

        // the unused ones are removed
        install_hooks_in(dir.path(), &[Trigger::PreCommit], false).unwrap();
        assert!(dir.path().join("pre-commit").exists());
        assert!(!dir.path().join("commit-msg").exists());
    }

    #[test]
    fn keep_user_hooks_unless_forced() {
        let dir = tempdir().unwrap();
        let pre_commit = dir.path().join("pre-commit");
        fs::write(&pre_commit, "#!/bin/sh\necho mine\n").unwrap();

        let skipped = install_hooks_in(dir.path(), &Trigger::ALL, false).unwrap();
        assert_eq!(skipped, vec![pre_commit.clone()]);
        assert!(
            fs::read_to_string(&pre_commit)
                .unwrap()
                .contains("echo mine")
        );

        install_hooks_in(dir.path(), &Trigger::ALL, true).unwrap();
        assert!(
            fs::read_to_string(&pre_commit)
                .unwrap()
                .contains(HOOK_MARKER)
        );
    }

    #[test]
    fn hooks_follow_the_config() {
        let dir = tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        std::process::Command::new("git")
            .args(["init", "-q"])
            .current_dir(&root)
            .status()
            .unwrap();
        fs::write(
            root.join(crate::config::JSON_CONFIG_FILENAME),
            r#"{ "hooks": [ { "name": "msg", "trigger": "commit-msg", "cmd": "true" } ] }"#,
        )
        .unwrap();

        assert!(!is_installed(&root));
        install_hooks(&root, false).unwrap();
        assert!(is_installed(&root));
        let hooks = root.join(".git/hooks");
        assert!(hooks.join("commit-msg").exists());
        assert!(hooks.join("pre-commit").exists());
        assert!(!hooks.join("pre-push").exists());

        // a pre-push hook is added to the config: installed on the next run
        let configs: Vec<Config> = vec![
            serde_json::from_str(
                r#"{ "hooks": [ { "name": "test", "trigger": "pre-push", "cmd": "true" } ] }"#,
            )
            .unwrap(),
        ];
        sync(&root, &configs).unwrap();
        assert!(hooks.join("pre-push").exists());
        assert!(!hooks.join("commit-msg").exists());
    }
}
