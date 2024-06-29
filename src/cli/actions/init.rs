use std::io;

use colored::Colorize;

use crate::config::{AppConfig, HOOK_CONFIG_FILENAME};

/// create `hooker.config.yaml`
pub fn init_configuration() -> io::Result<()> {
    if AppConfig::exists()? {
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
    new_config.write_config()?;

    Ok(())
}
