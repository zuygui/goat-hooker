use std::{
    io::{IsTerminal, Read},
    path::{Path, PathBuf},
};

use colored::Colorize;

use goat_hooker::{
    config::{ConfigLoader, HOOK_CONFIG_FILENAME, Trigger},
    errors::GoatHookerError,
    git::{self, UnstagedStash},
    matcher::{MatchedHook, Matcher},
    runner::{self, HookInput},
};

/// Config explicite, sinon celle à la racine du dépôt (`None` si elle n'existe pas)
pub fn resolve_config(root: &Path, config_path: Option<PathBuf>) -> Option<PathBuf> {
    match config_path {
        Some(path) => Some(path),
        None => Some(root.join(HOOK_CONFIG_FILENAME)).filter(|p| p.exists()),
    }
}

pub fn run_hooks(
    trigger: Trigger,
    config_path: Option<PathBuf>,
    args: &[String],
    dry_run: bool,
) -> Result<(), GoatHookerError> {
    // `HOOKER_SKIP=1 git commit` pour passer les hooks
    if std::env::var(git::SKIP_ENV).is_ok_and(|v| !v.is_empty() && v != "0") {
        return Ok(());
    }

    let cwd = std::env::current_dir()?;
    let root = git::repo_root(&cwd)?;

    // Si la config n'existe pas, on ne bloque pas Git
    let Some(config_path) = resolve_config(&root, config_path) else {
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
    };

    let configs = ConfigLoader::new().load_recursive(&config_path)?;
    let parallel = configs.first().and_then(|c| c.parallel).unwrap_or(false);
    let matcher = Matcher::new(&configs)?;

    // Git donne au pre-push les refs poussées sur stdin.
    // On le lit pour filtrer les hooks, puis on le redonne à chaque commande.
    let input = HookInput {
        args: args.to_vec(),
        stdin: read_stdin(trigger)?,
    };

    let files = runner::changed_files(trigger, &root, &input)?;
    let hooks = matcher.hooks_to_run(trigger, files.as_deref());
    if hooks.is_empty() {
        println!(
            "{}",
            format!("🐐 No {} hooks to run", trigger.as_str()).dimmed()
        );
        return Ok(());
    }

    if dry_run {
        print_dry_run(&hooks, &root);
        return Ok(());
    }

    // Les hooks du pre-commit doivent voir exactement ce qui va être commité
    let mut stash = if trigger == Trigger::PreCommit {
        UnstagedStash::save(&root)?
    } else {
        None
    };

    let results = runner::run_all(&hooks, &input, parallel)?;

    // Re-stage les fichiers corrigés (formateurs) avant de remettre les modifs non stagées
    let files_to_stage: Vec<PathBuf> = results
        .iter()
        .filter(|r| r.success)
        .flat_map(|r| r.files_to_stage.clone())
        .collect();
    git::add(&root, &files_to_stage)?;

    if let Some(stash) = stash.as_mut() {
        stash.restore()?;
    }

    let failed: Vec<&str> = results
        .iter()
        .filter(|r| !r.success)
        .map(|r| r.name.as_str())
        .collect();
    if !failed.is_empty() {
        return Err(GoatHookerError::HookFailed(failed.join("`, `")));
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

fn print_dry_run(hooks: &[MatchedHook], root: &Path) {
    for matched in hooks {
        let project = matched
            .base_dir
            .strip_prefix(root)
            .unwrap_or(&matched.base_dir);
        let project = if project.as_os_str().is_empty() {
            ".".to_string()
        } else {
            project.display().to_string()
        };

        println!(
            "{} {}",
            format!("🐐 {}", matched.hook.name).bold(),
            format!("({project})").dimmed()
        );
        for cmd in matched.hook.commands() {
            println!("   🚧 {cmd}");
        }
        for file in &matched.files {
            println!("   📄 {}", file.display());
        }
    }
}
