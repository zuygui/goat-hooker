use std::{
    fs::{self, create_dir_all},
    path::Path,
};

use colored::Colorize;

use goat_hooker::{config::Trigger, errors::GoatHookerError, git};

/// Marque les hooks installés par goat-hooker (pour ne pas écraser ceux de l'utilisateur)
const HOOK_MARKER: &str = "# installed by goat-hooker";

pub fn install_hooks(force: bool) -> Result<(), GoatHookerError> {
    let cwd = std::env::current_dir()?;
    let root = git::repo_root(&cwd)?;
    let hooks_dir = git::hooks_dir(&root)?;

    install_hooks_in(&hooks_dir, force)?;

    println!("{}", "✅ Hooks installed successfully".green());
    Ok(())
}

fn install_hooks_in(hooks_dir: &Path, force: bool) -> Result<(), GoatHookerError> {
    // Crée le répertoire des hooks si nécessaire
    create_dir_all(hooks_dir)?;

    for trigger in Trigger::ALL {
        let hook_name = trigger.as_str();
        let hook_path = hooks_dir.join(hook_name);

        // Ne pas écraser un hook écrit à la main
        if hook_path.exists() && !force {
            let content = fs::read_to_string(&hook_path).unwrap_or_default();
            if !content.contains(HOOK_MARKER) {
                eprintln!(
                    "{}",
                    format!(
                        "⚠️ Warning: {} already exists and wasn't installed by goat-hooker. Skipping (use --force to overwrite).",
                        hook_path.display()
                    )
                    .yellow()
                );
                continue;
            }
        }

        let script = format!("#!/bin/sh\n{HOOK_MARKER}\nexec hooker run {hook_name} -- \"$@\"\n");
        fs::write(&hook_path, script)?;

        // Rendre le fichier exécutable (chmod 755)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&hook_path, fs::Permissions::from_mode(0o755))?;
        }
    }

    Ok(())
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

        install_hooks_in(dir.path(), false).unwrap();
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
}
