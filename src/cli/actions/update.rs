use std::path::PathBuf;

use colored::Colorize;

use goat_hooker::{config::ConfigLoader, errors::GoatHookerError, git};

use super::run::resolve_config;

/// Updates the `from` repositories in cache
pub fn update_remotes(config_path: Option<PathBuf>) -> Result<(), GoatHookerError> {
    let cwd = std::env::current_dir()?;
    let root = git::repo_root(&cwd)?;
    let Some(config_path) = resolve_config(&root, config_path) else {
        return Err(GoatHookerError::ConfigNotFound(
            goat_hooker::config::config_path(&root),
        ));
    };

    ConfigLoader::new()
        .refresh_remotes()
        .load_recursive(&config_path)?;

    println!("{}", "✅ Remote hooks updated".green());
    Ok(())
}
