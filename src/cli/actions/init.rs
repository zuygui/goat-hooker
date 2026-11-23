use std::{io, path::Path};

use colored::Colorize;

use goat_hooker::{
    config::{Config, HOOK_CONFIG_FILENAME, SCHEMA_URL},
    errors::GoatHookerError,
};

/// Creates `hooker.config.json` in the given folder
pub fn init_configuration(dir: &Path) -> Result<(), GoatHookerError> {
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

    let new_config = Config {
        schema: Some(SCHEMA_URL.to_string()),
        hooks: Some(vec![]),
        ..Default::default()
    };
    new_config.write_to_dir(dir)?;

    println!(
        "{}",
        format!(
            "✅ {} file created!",
            HOOK_CONFIG_FILENAME.italic().yellow()
        )
        .green()
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use goat_hooker::config::ConfigLoader;
    use tempfile::tempdir;

    #[test]
    fn created_config_is_loadable() {
        let dir = tempdir().unwrap();
        init_configuration(dir.path()).unwrap();

        let mut loader = ConfigLoader::new();
        let cfgs = loader
            .load_recursive(&dir.path().join(HOOK_CONFIG_FILENAME))
            .unwrap();
        assert_eq!(cfgs.len(), 1);
    }

    #[test]
    fn existing_config_is_kept() {
        let dir = tempdir().unwrap();
        let path = dir.path().join(HOOK_CONFIG_FILENAME);
        std::fs::write(&path, "custom").unwrap();

        init_configuration(dir.path()).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "custom");
    }
}
