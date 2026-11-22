use std::{
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use colored::Colorize;

use crate::{
    config::{ConfigLoader, HOOK_CONFIG_FILENAME, Trigger},
    errors::GoatHookerError,
    git,
    matcher::{MatchedHook, Matcher},
};

pub fn run_hooks(
    trigger: Trigger,
    config_path: Option<PathBuf>,
    args: &[String],
) -> Result<(), GoatHookerError> {
    let cwd = std::env::current_dir()?;
    let root = git::repo_root(&cwd)?;

    // Sans config explicite, on prend celle à la racine du dépôt.
    // Si elle n'existe pas, on ne bloque pas Git.
    let config_path = match config_path {
        Some(path) => path,
        None => {
            let path = root.join(HOOK_CONFIG_FILENAME);
            if !path.exists() {
                eprintln!(
                    "{}",
                    format!(
                        "⚠️ Warning: no {} found at {}, skipping {} hooks",
                        HOOK_CONFIG_FILENAME,
                        root.display(),
                        trigger.as_str()
                    )
                    .yellow()
                );
                return Ok(());
            }
            path
        }
    };

    let configs = ConfigLoader::new().load_recursive(&config_path)?;
    let matcher = Matcher::new(&configs)?;

    // Pour le pre-commit, on ne lance que les hooks qui concernent les fichiers stagés
    let files = if trigger.uses_staged_files() {
        Some(git::staged_files(&root)?)
    } else {
        None
    };

    let hooks = matcher.hooks_to_run(trigger, files.as_deref());
    if hooks.is_empty() {
        println!(
            "{}",
            format!("🐐 No {} hooks to run", trigger.as_str()).dimmed()
        );
        return Ok(());
    }

    for matched in hooks {
        run_hook(&matched, args)?;
    }

    Ok(())
}

fn run_hook(matched: &MatchedHook, args: &[String]) -> Result<(), GoatHookerError> {
    let hook = matched.hook;
    let commands = hook.commands();

    if commands.is_empty() {
        eprintln!(
            "{}",
            format!(
                "⚠️ Warning: hook `{}` uses `from`/`path`, which isn't supported yet. Skipping.",
                hook.name
            )
            .yellow()
        );
        return Ok(());
    }

    println!("{}", format!("🐐 Running hook `{}`", hook.name).bold());
    for cmd in commands {
        if !execute_command(cmd, &matched.base_dir, args)? {
            return Err(GoatHookerError::HookFailed(hook.name.clone()));
        }
    }

    Ok(())
}

/// Exécute une commande shell depuis `dir`.
/// Les arguments de Git sont passés en paramètres positionnels (`$1`, `$2`...).
fn execute_command(cmd: &str, dir: &Path, args: &[String]) -> Result<bool, GoatHookerError> {
    println!("🚧 Executing command: `{cmd}`");

    let status = Command::new("sh")
        .arg("-c")
        .arg(cmd)
        .arg("hooker") // $0
        .args(args)
        .current_dir(dir)
        .stdout(Stdio::inherit()) // Rediriger la sortie standard vers le processus parent
        .stderr(Stdio::inherit()) // Rediriger la sortie d'erreur vers le processus parent
        .status()?;

    if status.success() {
        println!("{}", "✅ Command executed successfully".green());
    } else {
        eprintln!("{}", format!("❌ Command failed with {status}").red());
    }

    Ok(status.success())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn command_status() {
        let dir = tempdir().unwrap();
        assert!(execute_command("true", dir.path(), &[]).unwrap());
        assert!(!execute_command("exit 3", dir.path(), &[]).unwrap());
    }

    #[test]
    fn command_gets_git_args_and_runs_in_base_dir() {
        let dir = tempdir().unwrap();
        let args = vec!["MSG_FILE".to_string()];

        assert!(execute_command(r#"echo "$1" > out.txt"#, dir.path(), &args).unwrap());
        assert_eq!(
            std::fs::read_to_string(dir.path().join("out.txt")).unwrap(),
            "MSG_FILE\n"
        );
    }
}
