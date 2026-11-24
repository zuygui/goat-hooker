use std::{collections::BTreeMap, path::PathBuf};

use colored::Colorize;
use inquire::{Confirm, Select, Text};

use goat_hooker::{
    config::ConfigLoader,
    errors::GoatHookerError,
    hub::{self, SharedFile, SharedRepo},
};

use super::add::prompt_error;
use crate::cli::ui;

/// Searches hooks shared on GitHub, and adds the picked hooks file
pub fn search_hooks(query: &str, project: Option<PathBuf>) -> Result<(), GoatHookerError> {
    eprintln!("{} Searching hooks shared on GitHub…", "🔎".bold());
    let (repos, note) = hub::search(query)?;
    if let Some(note) = note {
        eprintln!("{} {note}", "!".yellow().bold());
    }
    if repos.is_empty() {
        println!("{} No shared hooks match `{query}`", ui::PENDING.dimmed());
        return Ok(());
    }

    let mut choices: Vec<(&SharedRepo, &SharedFile)> = Vec::new();
    for repo in &repos {
        println!(
            "\n{} {}",
            repo.name.bold(),
            repo.description.as_deref().unwrap_or_default().dimmed()
        );
        for file in &repo.files {
            println!(
                "  {} {}",
                file.path.cyan(),
                file.description.as_deref().unwrap_or_default()
            );
            let hooks: Vec<String> = file
                .hooks
                .iter()
                .map(|h| match h.trigger {
                    Some(t) => format!("{} ({})", h.name, ui::trigger(t)),
                    None => h.name.clone(),
                })
                .collect();
            println!("    {}", hooks.join(", ").dimmed());
            choices.push((repo, file));
        }
    }
    println!();

    if !ui::is_interactive() {
        println!(
            "Add one with {}",
            ui::command("\"from\": \"https://github.com/<repo>\", \"path\": \"<file>\"")
        );
        return Ok(());
    }

    let labels: Vec<String> = choices
        .iter()
        .map(|(repo, file)| format!("{} › {}", repo.name, file.path))
        .collect();
    let Ok(choice) = Select::new("Add hooks file", labels.clone())
        .with_help_message("↑↓ to move, enter to pick, esc to quit")
        .prompt()
    else {
        return Ok(());
    };
    let (repo, file) = choices[labels.iter().position(|l| *l == choice).unwrap()];

    // What it will run
    println!();
    for hook in &file.hooks {
        println!(
            "  {} {}",
            hook.trigger.map(ui::trigger).unwrap_or_default(),
            hook.name.bold()
        );
        for cmd in &hook.commands {
            println!("    {} {cmd}", "$".dimmed());
        }
    }
    println!();

    let mut with = BTreeMap::new();
    for (name, param) in &file.params {
        let default = param
            .default
            .as_ref()
            .map(|d| match d {
                serde_json::Value::String(s) => s.clone(),
                other => other.to_string(),
            })
            .unwrap_or_default();
        let mut prompt = Text::new(name).with_default(&default);
        if let Some(description) = param.description.as_deref() {
            prompt = prompt.with_help_message(description);
        }
        let value = prompt.prompt().map_err(prompt_error)?;
        if value != default {
            // Keeps numbers as numbers
            let value = serde_json::from_str::<serde_json::Value>(&value)
                .ok()
                .filter(|v| v.is_number() || v.is_boolean())
                .unwrap_or(serde_json::Value::String(value));
            with.insert(name.clone(), value);
        }
    }
    let name = Text::new("Hook name")
        .with_default(&file.default_name())
        .prompt()
        .map_err(prompt_error)?;

    let root = ui::current_repo()?;
    let config_file = match project {
        Some(dir) => goat_hooker::config::config_path(&dir.canonicalize()?),
        None => goat_hooker::config::config_path(&root),
    };
    if !config_file.exists() {
        return Err(GoatHookerError::ConfigNotFound(config_file));
    }
    if !Confirm::new(&format!(
        "Add `{name}` to {} and trust these hooks?",
        config_file
            .strip_prefix(&root)
            .unwrap_or(&config_file)
            .display()
    ))
    .with_default(true)
    .prompt()
    .map_err(prompt_error)?
    {
        return Err(GoatHookerError::Cancelled);
    }

    hub::add(&root, &config_file, &name, repo, file, with)?;
    // Checks the whole config still loads
    ConfigLoader::new().load_recursive(&goat_hooker::config::config_path(&root))?;
    println!(
        "{} Added {} ({} hooks from {})",
        ui::OK.green(),
        name.bold(),
        file.hooks.len(),
        repo.name
    );
    Ok(())
}
