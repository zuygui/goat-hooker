use colored::Colorize;

use goat_hooker::{
    config::{ConfigLoader, HOOK_CONFIG_FILENAME},
    errors::GoatHookerError,
    variables,
};

use super::run::resolve_config;
use crate::cli::ui;

/// Lists the variables shared between hooks, or resets one
pub fn variables(reset: Option<String>) -> Result<(), GoatHookerError> {
    let root = ui::current_repo()?;
    if let Some(name) = reset {
        variables::reset(&root, &name)?;
        println!("{} `{name}` is back to its default value", ui::OK.green());
        return Ok(());
    }
    let Some(config_path) = resolve_config(&root, None) else {
        return Err(GoatHookerError::ConfigNotFound(
            root.join(HOOK_CONFIG_FILENAME),
        ));
    };
    let configs = ConfigLoader::new().load_recursive(&config_path)?;
    let usages = variables::usage(&root, &configs)?;
    if usages.is_empty() {
        println!(
            "{} No variables, declare them in {} of hooker.config.json",
            ui::PENDING.dimmed(),
            "variables".bold()
        );
        return Ok(());
    }
    for usage in usages {
        let variable = &usage.declared.variable;
        println!(
            "{} {} {}",
            "$".dimmed(),
            usage.declared.name.bold(),
            match usage.value.as_deref() {
                Some(value) => format!("= {value}").cyan(),
                None => "(no value)".dimmed(),
            }
        );
        if let Some(description) = variable.description.as_ref() {
            println!("   {}", description.dimmed());
        }
        let list = |hooks: &[String]| {
            if hooks.is_empty() {
                "nobody".dimmed().to_string()
            } else {
                hooks.join(", ")
            }
        };
        let allowed = |list: Option<&Vec<String>>| {
            list.map(|l| format!(" (allowed: {})", l.join(", ")).dimmed().to_string())
                .unwrap_or_default()
        };
        println!(
            "   written by {}{}",
            list(&usage.writers),
            allowed(variable.writers.as_ref())
        );
        println!(
            "   read by    {}{}",
            list(&usage.readers),
            allowed(variable.readers.as_ref())
        );
        for violation in &usage.violations {
            println!("   {} {violation}", ui::KO.red());
        }
    }
    Ok(())
}
