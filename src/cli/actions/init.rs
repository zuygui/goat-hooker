use std::{io, path::Path};

use colored::Colorize;

use goat_hooker::{
    config::HOOK_CONFIG_FILENAME,
    errors::GoatHookerError,
    templates::{self, TEMPLATES, Template},
};

use crate::cli::{actions::add::prompt_error, ui};

/// Creates `hooker.config.json` in the given folder, with the hooks of templates
pub fn init_configuration(dir: &Path, template: &[String]) -> Result<(), GoatHookerError> {
    if !dir.is_dir() {
        return Err(GoatHookerError::ReadError(io::Error::new(
            io::ErrorKind::NotADirectory,
            format!(
                "Provided path `{}` isn't a directory",
                dir.to_string_lossy()
            ),
        )));
    }

    if dir.join(HOOK_CONFIG_FILENAME).exists() {
        println!(
            "{} {} already exists",
            "·".dimmed(),
            HOOK_CONFIG_FILENAME.bold()
        );
        return Ok(());
    }

    let chosen = choose_templates(dir, template)?;
    let new_config = templates::config_with(&chosen)?;
    new_config.write_to_dir(dir)?;

    println!(
        "{}",
        format!(
            "✅ {} file created!",
            HOOK_CONFIG_FILENAME.italic().yellow()
        )
        .green()
    );
    for template in &chosen {
        let hooks: Vec<String> = template.hooks()?.into_iter().map(|h| h.name).collect();
        println!(
            "   {} {}",
            template.name.bold(),
            format!("· {}", hooks.join(", ")).dimmed()
        );
    }
    if !chosen.is_empty() {
        println!(
            "   {} {}",
            "→ then run".dimmed(),
            ui::command("hooker install")
        );
    }

    Ok(())
}

/// Templates given with `--template`, else asked in a terminal, else none
fn choose_templates(
    dir: &Path,
    names: &[String],
) -> Result<Vec<&'static Template>, GoatHookerError> {
    let detected = templates::detect(dir);
    if names.iter().any(|n| n == "auto") {
        return Ok(detected);
    }
    if !names.is_empty() {
        return names
            .iter()
            .map(|name| {
                Template::find(name).ok_or_else(|| {
                    GoatHookerError::ReadError(io::Error::new(
                        io::ErrorKind::NotFound,
                        format!(
                            "unknown template `{name}` (templates: {})",
                            TEMPLATES
                                .iter()
                                .map(|t| t.name)
                                .collect::<Vec<_>>()
                                .join(", ")
                        ),
                    ))
                })
            })
            .collect();
    }
    if !ui::is_interactive() {
        return Ok(vec![]);
    }

    let options: Vec<String> = TEMPLATES
        .iter()
        .map(|t| format!("{:<11} {}", t.name, t.description()))
        .collect();
    let defaults: Vec<usize> = TEMPLATES
        .iter()
        .enumerate()
        .filter(|(_, t)| detected.iter().any(|d| d.name == t.name))
        .map(|(i, _)| i)
        .collect();
    let picked = inquire::MultiSelect::new("Hooks to start with", options.clone())
        .with_default(&defaults)
        .with_help_message("space: pick, enter: create (the ones found in this project are picked)")
        .prompt()
        .map_err(prompt_error)?;
    Ok(TEMPLATES
        .iter()
        .zip(&options)
        .filter(|(_, label)| picked.contains(label))
        .map(|(t, _)| t)
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use goat_hooker::config::ConfigLoader;
    use tempfile::tempdir;

    #[test]
    fn created_config_is_loadable() {
        let dir = tempdir().unwrap();
        init_configuration(dir.path(), &[]).unwrap();

        let mut loader = ConfigLoader::new();
        let cfgs = loader
            .load_recursive(&dir.path().join(HOOK_CONFIG_FILENAME))
            .unwrap();
        assert_eq!(cfgs.len(), 1);
    }

    #[test]
    fn templates_fill_the_config() {
        let dir = tempdir().unwrap();
        init_configuration(dir.path(), &["rust".into(), "commits".into()]).unwrap();
        let cfgs = ConfigLoader::new()
            .load_recursive(&dir.path().join(HOOK_CONFIG_FILENAME))
            .unwrap();
        let names: Vec<&str> = cfgs[0]
            .hooks
            .iter()
            .flatten()
            .map(|h| h.name.as_str())
            .collect();
        assert_eq!(
            names,
            ["rustfmt", "clippy", "cargo test", "conventional commit"]
        );
    }

    #[test]
    fn existing_config_is_kept() {
        let dir = tempdir().unwrap();
        let path = dir.path().join(HOOK_CONFIG_FILENAME);
        std::fs::write(&path, "custom").unwrap();

        init_configuration(dir.path(), &[]).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "custom");
    }
}
