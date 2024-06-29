use std::{io, path::PathBuf};

use colored::Colorize;

use crate::config::{AppConfig, HOOK_CONFIG_FILENAME};

/// create `hooker.config.yaml`
pub fn init_configuration(path: Option<PathBuf>) -> io::Result<()> {
    let p = path.unwrap_or(".".into());

    if !p.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            format!(
                "❌ Provided path `{}` isn't a directory",
                p.to_string_lossy()
            ),
        ));
    }

    if AppConfig::exists() {
        println!(
            "{}",
            format!(
                "🚀 {} file already exists. Skipping.",
                HOOK_CONFIG_FILENAME.italic()
            )
            .blue()
        );
        return Ok(());
    }

    let new_config = AppConfig::default();
    new_config.write_config(p)?;

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
