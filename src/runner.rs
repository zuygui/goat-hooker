use std::{
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::Mutex,
    time::{Duration, Instant},
};

use colored::Colorize;

use crate::{
    config::{Config, Trigger},
    errors::GoatHookerError,
    git::{self, UnstagedStash},
    matcher::{MatchedHook, Matcher},
};

/// Remplacé dans les commandes par les fichiers concernés (échappés pour le shell)
pub const FILES_PLACEHOLDER: &str = "{files}";

/// Ce que Git donne au hook
#[derive(Debug, Clone, Default)]
pub struct HookInput {
    /// Arguments du hook (`$1`, `$2`...)
    pub args: Vec<String>,
    /// stdin (refs du pre-push)
    pub stdin: Option<String>,
}

/// Où va la sortie des commandes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Output {
    /// Directement dans le terminal, avec les messages de progression (CLI)
    Inherit,
    /// Capturée et retournée dans le résultat, sans rien afficher (parallèle, GUI)
    Capture,
}

#[derive(Debug, Clone)]
pub struct CommandResult {
    pub cmd: String,
    pub success: bool,
    /// Sortie (stdout + stderr) en mode `Capture`
    pub output: String,
}

#[derive(Debug, Clone)]
pub struct HookResult {
    pub name: String,
    /// Dossier du projet du hook
    pub base_dir: PathBuf,
    pub success: bool,
    pub duration: Duration,
    pub commands: Vec<CommandResult>,
    /// Fichiers à re-stager (`stage_fixed`), en chemins absolus
    pub files_to_stage: Vec<PathBuf>,
}

/// Fichiers sur lesquels filtrer les hooks :
/// fichiers stagés pour le pre-commit, fichiers des commits poussés pour le pre-push.
/// `None` → les autres triggers lancent tous leurs hooks.
pub fn changed_files(
    trigger: Trigger,
    root: &Path,
    input: &HookInput,
) -> Result<Option<Vec<PathBuf>>, GoatHookerError> {
    match (trigger, input.stdin.as_deref()) {
        (Trigger::PreCommit, _) => Ok(Some(git::staged_files(root)?)),
        (Trigger::PrePush, Some(refs)) => {
            let remote = input.args.first().map(String::as_str).unwrap_or_default();
            Ok(Some(git::pushed_files(root, remote, refs)?))
        }
        _ => Ok(None),
    }
}

/// Lance les hooks, en parallèle ou les uns après les autres.
/// En séquentiel, on s'arrête au premier hook qui échoue.
/// En parallèle avec `Output::Inherit`, chaque hook est affiché quand il a fini.
pub fn run_all(
    hooks: &[MatchedHook],
    input: &HookInput,
    parallel: bool,
    output: Output,
) -> Result<Vec<HookResult>, GoatHookerError> {
    if !parallel {
        let mut results = Vec::new();
        for matched in hooks {
            let result = run_hook(matched, input, output)?;
            let failed = !result.success;
            results.push(result);
            if failed {
                break;
            }
        }
        return Ok(results);
    }

    // Les sorties sont capturées puis affichées hook par hook, pour ne pas les mélanger
    let print_lock = Mutex::new(());
    std::thread::scope(|scope| {
        let handles: Vec<_> = hooks
            .iter()
            .map(|matched| {
                let print_lock = &print_lock;
                scope.spawn(move || {
                    let result = run_hook(matched, input, Output::Capture)?;
                    if output == Output::Inherit {
                        let _guard = print_lock.lock().unwrap();
                        print_result(&result);
                    }
                    Ok(result)
                })
            })
            .collect();

        handles
            .into_iter()
            .map(|handle| handle.join().expect("hook thread panicked"))
            .collect()
    })
}

/// Lance tous les hooks d'un trigger, comme le ferait Git :
/// filtre sur les fichiers, met de côté les modifs non stagées (pre-commit),
/// re-stage les fichiers corrigés (`stage_fixed`).
pub fn run_trigger(
    root: &Path,
    configs: &[Config],
    trigger: Trigger,
    input: &HookInput,
    output: Output,
) -> Result<Vec<HookResult>, GoatHookerError> {
    let parallel = configs.first().and_then(|c| c.parallel).unwrap_or(false);
    let matcher = Matcher::new(configs)?;

    let files = changed_files(trigger, root, input)?;
    let hooks = matcher.hooks_to_run(trigger, files.as_deref());
    if hooks.is_empty() {
        return Ok(vec![]);
    }

    // Les hooks du pre-commit doivent voir exactement ce qui va être commité
    let mut stash = if trigger == Trigger::PreCommit {
        UnstagedStash::save(root)?
    } else {
        None
    };

    let results = run_all(&hooks, input, parallel, output)?;

    // Re-stage les fichiers corrigés (formateurs) avant de remettre les modifs non stagées
    let files_to_stage: Vec<PathBuf> = results
        .iter()
        .filter(|r| r.success)
        .flat_map(|r| r.files_to_stage.clone())
        .collect();
    git::add(root, &files_to_stage)?;

    if let Some(stash) = stash.as_mut() {
        stash.restore()?;
    }

    Ok(results)
}

/// Affiche le résultat d'un hook lancé en mode `Capture`
pub fn print_result(result: &HookResult) {
    println!("{}", format!("🐐 Hook `{}`", result.name).bold());
    for cmd in &result.commands {
        println!("🚧 Executing command: `{}`", cmd.cmd);
        print!("{}", cmd.output);
        if cmd.success {
            println!("{}", "✅ Command executed successfully".green());
        } else {
            eprintln!("{}", "❌ Command failed".red());
        }
    }
}

/// Lance les commandes d'un hook depuis le dossier de son projet
pub fn run_hook(
    matched: &MatchedHook,
    input: &HookInput,
    output: Output,
) -> Result<HookResult, GoatHookerError> {
    let hook = matched.hook;
    if output == Output::Inherit {
        println!("{}", format!("🐐 Running hook `{}`", hook.name).bold());
    }

    let start = Instant::now();
    let mut result = HookResult {
        name: hook.name.clone(),
        base_dir: matched.base_dir.clone(),
        success: true,
        duration: Duration::ZERO,
        commands: vec![],
        files_to_stage: vec![],
    };

    for cmd in hook.commands() {
        let cmd_result = execute_command(cmd, &matched.base_dir, input, &matched.files, output)?;
        let failed = !cmd_result.success;
        result.commands.push(cmd_result);
        if failed {
            result.success = false;
            result.duration = start.elapsed();
            return Ok(result);
        }
    }
    result.duration = start.elapsed();

    if hook.stage_fixed.unwrap_or(false) {
        result.files_to_stage = matched
            .files
            .iter()
            .map(|f| matched.base_dir.join(f))
            .filter(|f| f.exists())
            .collect();
    }

    Ok(result)
}

/// Échappe un chemin pour le shell (`it's.rs` → `'it'\''s.rs'`)
fn shell_quote(path: &Path) -> String {
    format!("'{}'", path.to_string_lossy().replace('\'', r"'\''"))
}

/// Exécute une commande shell depuis `dir`.
/// - les arguments de Git sont passés en paramètres positionnels (`$1`, `$2`...)
/// - `{files}` est remplacé par les fichiers concernés, aussi dispo dans `$HOOKER_FILES` (un par ligne)
/// - stdin est redonné à la commande (refs du pre-push)
fn execute_command(
    cmd: &str,
    dir: &Path,
    input: &HookInput,
    files: &[PathBuf],
    output: Output,
) -> Result<CommandResult, GoatHookerError> {
    if output == Output::Inherit {
        println!("🚧 Executing command: `{cmd}`");
    }

    let quoted_files: Vec<String> = files.iter().map(|f| shell_quote(f)).collect();
    let shell_cmd = cmd.replace(FILES_PLACEHOLDER, &quoted_files.join(" "));
    let files_env: Vec<String> = files
        .iter()
        .map(|f| f.to_string_lossy().to_string())
        .collect();

    let mut command = Command::new("sh");
    command
        .arg("-c")
        // stderr redirigé dans stdout pour garder l'ordre des messages en mode capture
        .arg(match output {
            Output::Inherit => shell_cmd,
            Output::Capture => format!("{{ {shell_cmd}\n}} 2>&1"),
        })
        .arg("hooker") // $0
        .args(&input.args)
        .current_dir(dir)
        .env("HOOKER_FILES", files_env.join("\n"))
        .stdin(if input.stdin.is_some() {
            Stdio::piped()
        } else if output == Output::Capture {
            Stdio::null()
        } else {
            Stdio::inherit()
        });

    match output {
        // Rediriger la sortie standard et d'erreur vers le processus parent
        Output::Inherit => command.stdout(Stdio::inherit()).stderr(Stdio::inherit()),
        Output::Capture => command.stdout(Stdio::piped()).stderr(Stdio::piped()),
    };

    let mut child = command.spawn()?;

    if let (Some(content), Some(mut child_stdin)) = (input.stdin.as_ref(), child.stdin.take()) {
        // La commande peut ne pas lire stdin : on ignore un pipe fermé
        let _ = child_stdin.write_all(content.as_bytes());
    }

    let out = child.wait_with_output()?;
    let success = out.status.success();

    if output == Output::Inherit {
        if success {
            println!("{}", "✅ Command executed successfully".green());
        } else {
            eprintln!("{}", format!("❌ Command failed with {}", out.status).red());
        }
    }

    let mut captured = String::from_utf8_lossy(&out.stdout).to_string();
    captured.push_str(&String::from_utf8_lossy(&out.stderr));

    Ok(CommandResult {
        cmd: cmd.to_string(),
        success,
        output: captured,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Hook;
    use tempfile::tempdir;

    fn read(dir: &Path, file: &str) -> String {
        std::fs::read_to_string(dir.join(file)).unwrap()
    }

    fn input(args: &[&str], stdin: Option<&str>) -> HookInput {
        HookInput {
            args: args.iter().map(|a| a.to_string()).collect(),
            stdin: stdin.map(String::from),
        }
    }

    fn hook(name: &str, cmd: &str) -> Hook {
        Hook {
            name: name.to_string(),
            trigger: Some(Trigger::PreCommit),
            cmd: Some(cmd.to_string()),
            ..Default::default()
        }
    }

    #[test]
    fn command_status() {
        let dir = tempdir().unwrap();
        let ok = execute_command("true", dir.path(), &input(&[], None), &[], Output::Inherit);
        assert!(ok.unwrap().success);
        let ko = execute_command(
            "exit 3",
            dir.path(),
            &input(&[], None),
            &[],
            Output::Inherit,
        );
        assert!(!ko.unwrap().success);
    }

    #[test]
    fn command_gets_git_args_and_runs_in_base_dir() {
        let dir = tempdir().unwrap();
        let res = execute_command(
            r#"echo "$1" > out.txt"#,
            dir.path(),
            &input(&["MSG_FILE"], None),
            &[],
            Output::Inherit,
        );
        assert!(res.unwrap().success);
        assert_eq!(read(dir.path(), "out.txt"), "MSG_FILE\n");
    }

    #[test]
    fn command_gets_files() {
        let dir = tempdir().unwrap();
        let files = vec![PathBuf::from("src/a.rs"), PathBuf::from("it's a file.rs")];

        // {files} est échappé : chaque fichier reste un seul argument
        let res = execute_command(
            r#"for f in {files}; do echo "$f"; done"#,
            dir.path(),
            &input(&[], None),
            &files,
            Output::Capture,
        )
        .unwrap();
        assert_eq!(res.output, "src/a.rs\nit's a file.rs\n");

        let res = execute_command(
            r#"echo "$HOOKER_FILES""#,
            dir.path(),
            &input(&[], None),
            &files,
            Output::Capture,
        )
        .unwrap();
        assert_eq!(res.output, "src/a.rs\nit's a file.rs\n");
    }

    #[test]
    fn command_gets_stdin() {
        let dir = tempdir().unwrap();
        let res = execute_command(
            "cat",
            dir.path(),
            &input(&[], Some("refs/heads/main abc\n")),
            &[],
            Output::Capture,
        )
        .unwrap();
        assert_eq!(res.output, "refs/heads/main abc\n");

        // une commande qui ne lit pas stdin ne doit pas planter
        let big = "x".repeat(1 << 20);
        let res = execute_command(
            "true",
            dir.path(),
            &input(&[], Some(&big)),
            &[],
            Output::Capture,
        );
        assert!(res.unwrap().success);
    }

    #[test]
    fn capture_keeps_stderr_in_order() {
        let dir = tempdir().unwrap();
        let res = execute_command(
            "echo a; echo b >&2; echo c",
            dir.path(),
            &input(&[], None),
            &[],
            Output::Capture,
        )
        .unwrap();
        assert_eq!(res.output, "a\nb\nc\n");
    }

    #[test]
    fn run_in_parallel() {
        let dir = tempdir().unwrap();
        let hooks = [
            hook("a", "sleep 0.3; echo a"),
            hook("b", "sleep 0.3; echo b"),
            hook("c", "exit 1"),
        ];
        let matched: Vec<MatchedHook> = hooks
            .iter()
            .map(|h| MatchedHook {
                hook: h,
                base_dir: dir.path().to_path_buf(),
                files: vec![],
            })
            .collect();

        let start = std::time::Instant::now();
        let results = run_all(&matched, &input(&[], None), true, Output::Capture).unwrap();
        assert!(
            start.elapsed().as_millis() < 550,
            "hooks didn't run in parallel"
        );

        // en parallèle, tous les hooks tournent même si l'un échoue
        assert_eq!(results.len(), 3);
        assert_eq!(results[0].commands[0].output, "a\n");
        assert!(!results[2].success);
    }

    #[test]
    fn stop_at_first_failure_in_sequence() {
        let dir = tempdir().unwrap();
        let hooks = [hook("fail", "exit 1"), hook("never", "true")];
        let matched: Vec<MatchedHook> = hooks
            .iter()
            .map(|h| MatchedHook {
                hook: h,
                base_dir: dir.path().to_path_buf(),
                files: vec![],
            })
            .collect();

        let results = run_all(&matched, &input(&[], None), false, Output::Capture).unwrap();
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn stage_fixed_files() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("a.rs"), "").unwrap();
        let mut fmt = hook("fmt", "true");
        fmt.stage_fixed = Some(true);
        let matched = MatchedHook {
            hook: &fmt,
            base_dir: dir.path().to_path_buf(),
            files: vec![PathBuf::from("a.rs"), PathBuf::from("deleted.rs")],
        };

        let result = run_hook(&matched, &input(&[], None), Output::Capture).unwrap();
        assert_eq!(result.files_to_stage, vec![dir.path().join("a.rs")]);
    }
}
