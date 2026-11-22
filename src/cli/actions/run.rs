use std::{
    io::{IsTerminal, Read, Write},
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

/// Remplacé dans les commandes par les fichiers concernés (échappés pour le shell)
const FILES_PLACEHOLDER: &str = "{files}";

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

    // Git donne au pre-push les refs poussées sur stdin.
    // On le lit pour filtrer les hooks, puis on le redonne à chaque commande.
    let stdin = read_stdin(trigger)?;

    // Pre-commit : fichiers stagés / pre-push : fichiers des commits poussés.
    // Les autres triggers lancent tous leurs hooks.
    let files = match (trigger, stdin.as_deref()) {
        (Trigger::PreCommit, _) => Some(git::staged_files(&root)?),
        (Trigger::PrePush, Some(refs)) => {
            let remote = args.first().map(String::as_str).unwrap_or_default();
            Some(git::pushed_files(&root, remote, refs)?)
        }
        _ => None,
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
        run_hook(&matched, args, stdin.as_deref())?;
    }

    Ok(())
}

/// Lit stdin pour le pre-push (sauf si lancé à la main depuis un terminal)
fn read_stdin(trigger: Trigger) -> Result<Option<String>, GoatHookerError> {
    if trigger != Trigger::PrePush || std::io::stdin().is_terminal() {
        return Ok(None);
    }

    let mut content = String::new();
    std::io::stdin().read_to_string(&mut content)?;
    Ok(Some(content))
}

fn run_hook(
    matched: &MatchedHook,
    args: &[String],
    stdin: Option<&str>,
) -> Result<(), GoatHookerError> {
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
        if !execute_command(cmd, &matched.base_dir, args, &matched.files, stdin)? {
            return Err(GoatHookerError::HookFailed(hook.name.clone()));
        }
    }

    Ok(())
}

/// Échappe un chemin pour le shell (`it's.rs` → `'it'\''s.rs'`)
fn shell_quote(path: &Path) -> String {
    format!("'{}'", path.to_string_lossy().replace('\'', r"'\''"))
}

/// Exécute une commande shell depuis `dir`.
/// - les arguments de Git sont passés en paramètres positionnels (`$1`, `$2`...)
/// - `{files}` est remplacé par les fichiers concernés, aussi dispo dans `$HOOKER_FILES` (un par ligne)
/// - `stdin` est redonné à la commande (refs du pre-push)
fn execute_command(
    cmd: &str,
    dir: &Path,
    args: &[String],
    files: &[PathBuf],
    stdin: Option<&str>,
) -> Result<bool, GoatHookerError> {
    println!("🚧 Executing command: `{cmd}`");

    let quoted_files: Vec<String> = files.iter().map(|f| shell_quote(f)).collect();
    let cmd = cmd.replace(FILES_PLACEHOLDER, &quoted_files.join(" "));
    let files_env: Vec<String> = files
        .iter()
        .map(|f| f.to_string_lossy().to_string())
        .collect();

    let mut child = Command::new("sh")
        .arg("-c")
        .arg(&cmd)
        .arg("hooker") // $0
        .args(args)
        .current_dir(dir)
        .env("HOOKER_FILES", files_env.join("\n"))
        .stdin(if stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::inherit()
        })
        .stdout(Stdio::inherit()) // Rediriger la sortie standard vers le processus parent
        .stderr(Stdio::inherit()) // Rediriger la sortie d'erreur vers le processus parent
        .spawn()?;

    if let (Some(content), Some(mut child_stdin)) = (stdin, child.stdin.take()) {
        // La commande peut ne pas lire stdin : on ignore un pipe fermé
        let _ = child_stdin.write_all(content.as_bytes());
    }

    let status = child.wait()?;
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

    fn read(dir: &Path, file: &str) -> String {
        std::fs::read_to_string(dir.join(file)).unwrap()
    }

    #[test]
    fn command_status() {
        let dir = tempdir().unwrap();
        assert!(execute_command("true", dir.path(), &[], &[], None).unwrap());
        assert!(!execute_command("exit 3", dir.path(), &[], &[], None).unwrap());
    }

    #[test]
    fn command_gets_git_args_and_runs_in_base_dir() {
        let dir = tempdir().unwrap();
        let args = vec!["MSG_FILE".to_string()];

        assert!(execute_command(r#"echo "$1" > out.txt"#, dir.path(), &args, &[], None).unwrap());
        assert_eq!(read(dir.path(), "out.txt"), "MSG_FILE\n");
    }

    #[test]
    fn command_gets_files() {
        let dir = tempdir().unwrap();
        let files = vec![PathBuf::from("src/a.rs"), PathBuf::from("it's a file.rs")];

        // {files} est échappé : chaque fichier reste un seul argument
        assert!(
            execute_command(
                r#"for f in {files}; do echo "$f"; done > out.txt"#,
                dir.path(),
                &[],
                &files,
                None
            )
            .unwrap()
        );
        assert_eq!(read(dir.path(), "out.txt"), "src/a.rs\nit's a file.rs\n");

        assert!(
            execute_command(
                r#"echo "$HOOKER_FILES" > env.txt"#,
                dir.path(),
                &[],
                &files,
                None
            )
            .unwrap()
        );
        assert_eq!(read(dir.path(), "env.txt"), "src/a.rs\nit's a file.rs\n");
    }

    #[test]
    fn command_gets_stdin() {
        let dir = tempdir().unwrap();

        assert!(
            execute_command(
                "cat > out.txt",
                dir.path(),
                &[],
                &[],
                Some("refs/heads/main abc\n")
            )
            .unwrap()
        );
        assert_eq!(read(dir.path(), "out.txt"), "refs/heads/main abc\n");

        // une commande qui ne lit pas stdin ne doit pas planter
        assert!(execute_command("true", dir.path(), &[], &[], Some(&"x".repeat(1 << 20))).unwrap());
    }
}
