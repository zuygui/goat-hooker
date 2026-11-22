use std::process;

use clap::Parser;
use colored::Colorize;

mod cli;

use goat_hooker::errors::GoatHookerError;

fn main() {
    let opt = cli::AppCli::parse();

    match cli::parse_commands(opt) {
        Ok(()) => {}
        // Le récap a déjà été affiché
        Err(GoatHookerError::HookFailed(_)) => process::exit(1),
        Err(GoatHookerError::Cancelled) => process::exit(130),
        Err(err) => {
            eprintln!("{} {}", "error:".red().bold(), err);
            if let Some(hint) = hint(&err) {
                eprintln!("  {} {hint}", "hint:".dimmed());
            }
            process::exit(1);
        }
    }
}

/// Piste de solution pour une erreur
fn hint(err: &GoatHookerError) -> Option<String> {
    let cmd = |c: &str| format!("`{c}`").bold().to_string();
    match err {
        GoatHookerError::ConfigNotFound(path) if path.ends_with("hooker.config.json") => {
            Some(format!("create one with {}", cmd("hooker init")))
        }
        GoatHookerError::ConfigNotFound(_) => {
            Some("check the `path` of your hooks files".to_string())
        }
        GoatHookerError::ParseError { .. } => Some(format!(
            "add {} to the file to get autocompletion and errors in your editor",
            cmd(
                "\"$schema\": \"https://raw.githubusercontent.com/zuygui/goat-hooker/main/schema.json\""
            )
        )),
        GoatHookerError::InvalidHook { reason, .. } if !reason.contains("hooker add") => {
            Some(format!("see every field with {}", cmd("hooker schema")))
        }
        GoatHookerError::InvalidPattern { .. } | GoatHookerError::InvalidImport { .. } => Some(
            "glob patterns use `*` (one folder), `**` (any folders), `?` and `[abc]`".to_string(),
        ),
        GoatHookerError::GitError(message) if message.contains("not a git repository") => {
            Some("run hooker inside a git repository (or `git init` first)".to_string())
        }
        GoatHookerError::GitError(message) if message.contains("clone") => Some(format!(
            "check the `from` URL and your access to it, then {}",
            cmd("hooker update")
        )),
        _ => None,
    }
}
