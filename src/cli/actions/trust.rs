use colored::Colorize;
use inquire::Confirm;

use goat_hooker::{
    config::{ConfigLoader, FileAccess, Hook, Origin},
    errors::GoatHookerError,
    trust,
};

use super::{add::prompt_error, run::resolve_config};
use crate::cli::ui;

/// Reviews and trusts the hooks loaded from other repositories
pub fn trust_hooks(all: bool) -> Result<(), GoatHookerError> {
    let root = ui::current_repo()?;
    let Some(config_path) = resolve_config(&root, None) else {
        return Err(GoatHookerError::ConfigNotFound(
            goat_hooker::config::config_path(&root),
        ));
    };
    let configs = ConfigLoader::new().load_recursive(&config_path)?;
    let hooks: Vec<&Hook> = configs
        .iter()
        .flat_map(|c| c.hooks.iter().flatten())
        .collect();

    let untrusted = trust::untrusted(&root, hooks.iter().copied());
    if untrusted.is_empty() {
        println!(
            "{} Every hook from other repositories is trusted",
            ui::OK.green()
        );
        return Ok(());
    }

    let mut approved: Vec<Origin> = Vec::new();
    for origin in &untrusted {
        print_origin(origin, &hooks);
        if let Some(changes) = trust::changes(&root, origin) {
            println!("   {}", "Changed since you trusted it:".yellow().bold());
            for change in changes {
                let line = if change.starts_with('!') {
                    change.red().to_string()
                } else {
                    change
                };
                println!("   {line}");
            }
        }
        let yes = if all {
            true
        } else if ui::is_interactive() {
            Confirm::new("Trust these hooks?")
                .with_default(false)
                .prompt()
                .map_err(prompt_error)?
        } else {
            println!(
                "{} run {} in a terminal to approve them, or {} to approve everything",
                "!".yellow().bold(),
                ui::command("hooker trust"),
                ui::command("hooker trust --all")
            );
            return Err(GoatHookerError::Cancelled);
        };
        if yes {
            approved.push(origin.clone());
        }
        println!();
    }

    trust::trust(&root, &approved)?;
    println!(
        "{} {} trusted, {} left",
        ui::OK.green(),
        ui::plural(approved.len(), "hooks file"),
        untrusted.len() - approved.len()
    );
    Ok(())
}

/// The hooks of a file of another repository, with what they can do
fn print_origin(origin: &Origin, hooks: &[&Hook]) {
    println!(
        "{} {} {}",
        "📦".bold(),
        origin.path.bold(),
        format!(
            "from {}{}",
            origin.url,
            origin
                .rev
                .as_ref()
                .map(|r| format!(" @ {r}"))
                .unwrap_or_default()
        )
        .dimmed()
    );
    for hook in hooks.iter().filter(|h| h.origin.as_ref() == Some(origin)) {
        let granted = hook.granted();
        let mut permissions = vec![match granted.files {
            FileAccess::Read => "read files".green(),
            FileAccess::Write => "modify files".yellow(),
        }];
        permissions.push(if granted.network {
            "network".yellow()
        } else {
            "no network".green()
        });
        match &granted.env {
            Some(env) if env.is_empty() => permissions.push("basic env".green()),
            Some(env) => permissions.push(format!("env: {}", env.join(", ")).yellow()),
            None => permissions.push("whole env".red()),
        }
        if let Some(writes) = hook.writes.as_ref().filter(|w| !w.is_empty()) {
            permissions.push(format!("writes {}", writes.join(", ")).normal());
        }
        println!(
            "   {} {} {}",
            hook.trigger.map(ui::trigger).unwrap_or_default(),
            hook.name.bold(),
            permissions
                .iter()
                .map(|p| p.to_string())
                .collect::<Vec<_>>()
                .join(" · ")
        );
        for cmd in hook.commands() {
            println!("      {} {}", "$".dimmed(), cmd);
        }
    }
}
