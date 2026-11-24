use colored::Colorize;

use goat_hooker::{
    config::{Config, ConfigLoader},
    errors::GoatHookerError,
    remote,
};

use crate::cli::ui;

/// Configs written in the repository (path, raw config)
fn raw_configs(
    root: &std::path::Path,
) -> Result<Vec<(std::path::PathBuf, Config)>, GoatHookerError> {
    let config_path = goat_hooker::config::config_path(root);
    if !config_path.exists() {
        return Err(GoatHookerError::ConfigNotFound(config_path));
    }
    ConfigLoader::new()
        .load_recursive(&config_path)?
        .into_iter()
        .filter_map(|c| c.file)
        .map(|file| Config::load_from_file(&file).map(|c| (file, c)))
        .collect()
}

fn short(sha: &str) -> String {
    sha.chars().take(8).collect()
}

/// Lists the hooks loaded from other repositories, and the ones with a newer version
pub fn outdated() -> Result<(), GoatHookerError> {
    let root = ui::current_repo()?;
    let mut found = false;
    for (file, config) in raw_configs(&root)? {
        for hook in config.hooks.iter().flatten() {
            let Some(url) = hook.from.as_deref() else {
                continue;
            };
            found = true;
            let rev = hook.rev.as_deref();
            let current = remote::head(&remote::repo_dir(url, rev));
            let status = if rev.is_some_and(remote::is_commit) {
                "pinned".dimmed().to_string()
            } else {
                match remote::latest(url, rev) {
                    Ok(Some(latest)) if current.as_deref() == Some(latest.as_str()) => {
                        "up to date".green().to_string()
                    }
                    Ok(Some(latest)) => {
                        format!("{} available", short(&latest)).yellow().to_string()
                    }
                    Ok(None) => "pinned".dimmed().to_string(),
                    Err(err) => err.to_string().red().to_string(),
                }
            };
            println!(
                "{} {} {} {} {}",
                hook.name.bold(),
                format!(
                    "{url}{}",
                    rev.map(|r| format!(" @ {r}")).unwrap_or_default()
                )
                .dimmed(),
                current
                    .as_deref()
                    .map(short)
                    .unwrap_or_else(|| "not fetched".into()),
                status,
                format!("({})", file.strip_prefix(&root).unwrap_or(&file).display()).dimmed()
            );
        }
    }
    if !found {
        println!(
            "{} No hooks loaded from other repositories",
            ui::PENDING.dimmed()
        );
    } else {
        println!(
            "\n{} to get the new versions, then {} to review them",
            ui::command("hooker update"),
            ui::command("hooker trust")
        );
    }
    Ok(())
}

/// Pins the hooks loaded from other repositories to the commit in use (`rev`)
pub fn pin(name: Option<String>) -> Result<(), GoatHookerError> {
    let root = ui::current_repo()?;
    let mut pinned = 0;
    for (file, mut config) in raw_configs(&root)? {
        let mut changed = false;
        for hook in config.hooks.iter_mut().flatten() {
            let Some(url) = hook.from.clone() else {
                continue;
            };
            if name.as_ref().is_some_and(|n| *n != hook.name)
                || hook.rev.as_deref().is_some_and(remote::is_commit)
            {
                continue;
            }
            let repo = remote::fetch(&url, hook.rev.as_deref())?;
            let Some(commit) = remote::head(&repo) else {
                continue;
            };
            println!(
                "{} {} pinned to {}",
                ui::OK.green(),
                hook.name.bold(),
                short(&commit).cyan()
            );
            hook.rev = Some(commit);
            changed = true;
            pinned += 1;
        }
        if changed {
            config.write_to_file(&file)?;
        }
    }
    if pinned == 0 {
        println!("{} Nothing to pin", ui::PENDING.dimmed());
    }
    Ok(())
}
