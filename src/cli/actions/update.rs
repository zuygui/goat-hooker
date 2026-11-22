use std::path::PathBuf;

use colored::Colorize;

use goat_hooker::{
    config::{ConfigLoader, HOOK_CONFIG_FILENAME},
    errors::GoatHookerError,
    git,
};

use super::run::resolve_config;

/// Met à jour les repos `from` en cache
pub fn update_remotes(config_path: Option<PathBuf>) -> Result<(), GoatHookerError> {
    let cwd = std::env::current_dir()?;
    let root = git::repo_root(&cwd)?;
    let Some(config_path) = resolve_config(&root, config_path) else {
        return Err(GoatHookerError::ConfigNotFound(
            root.join(HOOK_CONFIG_FILENAME),
        ));
    };

    ConfigLoader::new()
        .refresh_remotes()
        .load_recursive(&config_path)?;

    println!("{}", "✅ Remote hooks updated".green());
    Ok(())
}
