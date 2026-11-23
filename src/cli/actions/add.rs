use std::path::PathBuf;

use colored::Colorize;
use inquire::{Confirm, Select, Text, validator::Validation};

use goat_hooker::{
    config::{Config, ConfigLoader, HOOK_CONFIG_FILENAME, Hook, Trigger},
    errors::GoatHookerError,
    install,
};

use crate::cli::ui::{self, OK};

/// Ce qui a été donné en arguments (le reste est demandé dans le terminal)
#[derive(Debug, Clone, Default)]
pub struct AddArgs {
    pub name: Option<String>,
    pub trigger: Option<Trigger>,
    pub commands: Vec<String>,
    pub include: Vec<String>,
    pub exclude: Vec<String>,
    pub stage_fixed: bool,
    pub project: Option<PathBuf>,
}

/// Description affichée dans le choix du trigger
fn describe(trigger: Trigger) -> &'static str {
    match trigger {
        Trigger::PreCommit => "before a commit is created, on the staged files",
        Trigger::PrepareCommitMsg => "before the commit message editor opens",
        Trigger::CommitMsg => "to check the commit message ($1 = message file)",
        Trigger::PostCommit => "after a commit is created",
        Trigger::PreRebase => "before a rebase starts",
        Trigger::PostRewrite => "after an amend or a rebase",
        Trigger::PrePush => "before a push, on the pushed files",
        Trigger::ApplyPatchMsg => "to check the message of a patch (git am)",
        Trigger::PreApplyPatch => "before a patch is committed (git am)",
        Trigger::PostApplyPatch => "after a patch is applied (git am)",
        Trigger::PreAutoGc => "before git gc --auto",
        Trigger::PostCheckout => "after a checkout or a switch",
        Trigger::PostMerge => "after a merge or a pull",
    }
}

pub(crate) fn prompt_error(err: inquire::InquireError) -> GoatHookerError {
    match err {
        inquire::InquireError::OperationCanceled | inquire::InquireError::OperationInterrupted => {
            GoatHookerError::Cancelled
        }
        err => GoatHookerError::ReadError(std::io::Error::other(err.to_string())),
    }
}

/// Découpe `a, b,c` en patterns
fn split_patterns(values: &[String]) -> Vec<String> {
    values
        .iter()
        .flat_map(|v| v.split(','))
        .map(|p| p.trim().to_string())
        .filter(|p| !p.is_empty())
        .collect()
}

pub fn add_hook(args: AddArgs) -> Result<(), GoatHookerError> {
    let root = ui::current_repo()?;
    let config_path = root.join(HOOK_CONFIG_FILENAME);
    if !config_path.exists() {
        return Err(GoatHookerError::ConfigNotFound(config_path));
    }

    let configs = ConfigLoader::new().load_recursive(&config_path)?;
    let files: Vec<PathBuf> = configs.iter().filter_map(|c| c.file.clone()).collect();
    let interactive = ui::is_interactive();

    // Projet : argument, sinon choix, sinon celui du dossier courant
    let file = match args.project.as_ref() {
        Some(dir) => {
            let dir = dir.canonicalize()?;
            files
                .iter()
                .find(|f| f.parent() == Some(dir.as_path()))
                .cloned()
                .ok_or_else(|| GoatHookerError::ConfigNotFound(dir.join(HOOK_CONFIG_FILENAME)))?
        }
        None => {
            let current = project_of_cwd(&files).unwrap_or_else(|| files[0].clone());
            if interactive && files.len() > 1 {
                let labels: Vec<String> = files
                    .iter()
                    .map(|f| ui::project_label(&root, f.parent().unwrap()))
                    .collect();
                let start = files.iter().position(|f| *f == current).unwrap_or(0);
                let choice = Select::new("Project", labels.clone())
                    .with_starting_cursor(start)
                    .prompt()
                    .map_err(prompt_error)?;
                files[labels.iter().position(|l| *l == choice).unwrap()].clone()
            } else {
                current
            }
        }
    };

    let mut config = Config::load_from_file(&file)?;
    let existing: Vec<String> = config
        .hooks
        .iter()
        .flatten()
        .map(|h| h.name.clone())
        .collect();

    let missing = |what: &str| GoatHookerError::InvalidHook {
        name: args.name.clone().unwrap_or_default(),
        reason: format!("{what} is required (run `hooker add` in a terminal for the assistant)"),
    };

    let name = match args.name.clone() {
        Some(name) => name,
        None if interactive => {
            let existing = existing.clone();
            Text::new("Name")
                .with_placeholder("lint")
                .with_validator(move |name: &str| {
                    Ok(if name.trim().is_empty() {
                        Validation::Invalid("The hook needs a name".into())
                    } else if existing.iter().any(|e| e == name.trim()) {
                        Validation::Invalid("This project already has a hook with this name".into())
                    } else {
                        Validation::Valid
                    })
                })
                .prompt()
                .map_err(prompt_error)?
                .trim()
                .to_string()
        }
        None => return Err(missing("--name")),
    };
    if existing.contains(&name) {
        return Err(GoatHookerError::InvalidHook {
            name,
            reason: "this project already has a hook with this name".to_string(),
        });
    }

    let trigger = match args.trigger {
        Some(trigger) => trigger,
        None if interactive => {
            let options: Vec<String> = Trigger::ALL
                .iter()
                .map(|t| format!("{:<19} {}", t.as_str(), describe(*t).dimmed()))
                .collect();
            let choice = Select::new("Trigger", options.clone())
                .with_page_size(13)
                .with_formatter(&|option| {
                    option
                        .value
                        .split_whitespace()
                        .next()
                        .unwrap_or_default()
                        .to_string()
                })
                .prompt()
                .map_err(prompt_error)?;
            Trigger::ALL[options.iter().position(|o| *o == choice).unwrap()]
        }
        None => Trigger::PreCommit,
    };
    let uses_files = matches!(trigger, Trigger::PreCommit | Trigger::PrePush);

    let mut include = split_patterns(&args.include);
    let mut exclude = split_patterns(&args.exclude);
    if interactive && uses_files && args.include.is_empty() && args.commands.is_empty() {
        let answer = Text::new("Files")
            .with_placeholder("src/**/*.rs, *.toml")
            .with_help_message(
                "glob patterns relative to the project, comma separated · empty = all files",
            )
            .prompt()
            .map_err(prompt_error)?;
        include = split_patterns(&[answer]);

        let answer = Text::new("Ignored files")
            .with_placeholder("src/generated/**")
            .with_help_message("optional")
            .prompt()
            .map_err(prompt_error)?;
        exclude = split_patterns(&[answer]);
    }

    let mut commands = args.commands.clone();
    if commands.is_empty() {
        if !interactive {
            return Err(missing("--cmd"));
        }
        loop {
            let first = commands.is_empty();
            let command = Text::new(if first { "Command" } else { "Next command" })
                .with_placeholder(if uses_files { "cargo fmt --check -- {files}" } else { "" })
                .with_help_message(if first {
                    "run from the project folder · {files} = targeted files · $1, $2 = Git arguments"
                } else {
                    "empty to finish"
                })
                .prompt()
                .map_err(prompt_error)?;
            let command = command.trim().to_string();
            if command.is_empty() {
                if first {
                    continue;
                }
                break;
            }
            commands.push(command);
        }
    }

    let stage_fixed = if args.stage_fixed {
        true
    } else if interactive && trigger == Trigger::PreCommit && args.commands.is_empty() {
        Confirm::new("Re-stage the files after the hook?")
            .with_default(false)
            .with_help_message("for formatters that fix the files")
            .prompt()
            .map_err(prompt_error)?
    } else {
        false
    };

    let (cmd, cmds) = match commands.len() {
        1 => (commands.into_iter().next(), None),
        _ => (None, Some(commands)),
    };
    let hook = Hook {
        name: name.clone(),
        trigger: Some(trigger),
        include: (!include.is_empty()).then_some(include),
        exclude: (!exclude.is_empty()).then_some(exclude),
        cmd,
        cmds,
        stage_fixed: stage_fixed.then_some(true),
        ..Default::default()
    };
    hook.validate()?;

    config.hooks.get_or_insert_with(Vec::new).push(hook);
    config.write_to_file(&file)?;

    let rel = file.strip_prefix(root.canonicalize()?).unwrap_or(&file);
    println!(
        "{} Added {} ({}) to {}",
        OK.green(),
        name.bold(),
        ui::trigger(trigger),
        rel.display()
    );
    if !install::is_installed(&root) {
        println!(
            "  {} run {} so Git runs it",
            "→".dimmed(),
            ui::command("hooker install")
        );
    }
    Ok(())
}

/// Config du projet le plus proche du dossier courant
fn project_of_cwd(files: &[PathBuf]) -> Option<PathBuf> {
    let cwd = std::env::current_dir().ok()?.canonicalize().ok()?;
    files
        .iter()
        .filter(|f| f.parent().is_some_and(|dir| cwd.starts_with(dir)))
        .max_by_key(|f| f.components().count())
        .cloned()
}
