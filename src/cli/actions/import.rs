use colored::Colorize;

use goat_hooker::{errors::GoatHookerError, git, import};

use crate::cli::ui;

/// Converts the configs of husky / lint-staged, lefthook and pre-commit
pub fn import_configs(force: bool, dry_run: bool) -> Result<(), GoatHookerError> {
    let cwd = std::env::current_dir()?;
    let root = git::repo_root(&cwd)?;
    let imported = import::detect(&root)?;

    if imported.is_empty() {
        println!(
            "{} Nothing to import: no husky, lint-staged, lefthook or pre-commit config found",
            "·".dimmed()
        );
        return Ok(());
    }

    let sources: Vec<String> = imported
        .sources
        .iter()
        .map(|s| s.display().to_string())
        .collect();
    println!(
        "{} {} from {}",
        "🐐".bold(),
        ui::plural(imported.hooks_count(), "hook").bold(),
        sources.join(", ").cyan()
    );
    for (dir, config) in &imported.configs {
        let file = dir.join(goat_hooker::config::HOOK_CONFIG_FILENAME);
        println!("\n{}", file.display().to_string().bold());
        for hook in config.hooks.iter().flatten() {
            println!(
                "  {} {} {}",
                hook.trigger.map(ui::trigger).unwrap_or_default(),
                hook.name.bold(),
                ui::command(hook.cmd.as_deref().unwrap_or_default())
            );
        }
    }
    if !imported.notes.is_empty() {
        println!();
        for note in &imported.notes {
            println!("{} {note}", "!".yellow().bold());
        }
    }

    if dry_run {
        return Ok(());
    }
    import::write(&root, &imported, force)?;
    println!(
        "\n{} Written. Check the hooks with {} then install them with {}",
        "✔".green(),
        "hooker list".bold(),
        "hooker install".bold()
    );
    Ok(())
}
