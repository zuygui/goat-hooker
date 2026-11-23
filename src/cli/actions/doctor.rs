use colored::Colorize;

use goat_hooker::{
    config::{ConfigLoader, HOOK_CONFIG_FILENAME},
    doctor::{self, Level},
    errors::GoatHookerError,
};

use crate::cli::ui;

/// `hooker doctor`: what could go wrong, and how to fix it
pub fn check() -> Result<(), GoatHookerError> {
    let root = ui::current_repo()?;
    let config_path = root.join(HOOK_CONFIG_FILENAME);
    if !config_path.exists() {
        println!(
            "{} no {HOOK_CONFIG_FILENAME}  {} {}",
            "○".yellow(),
            "→ run".dimmed(),
            ui::command("hooker init")
        );
        return Ok(());
    }
    let configs = ConfigLoader::new().load_recursive(&config_path)?;
    let findings = doctor::diagnose(&root, &configs);

    if findings.is_empty() {
        println!("{} Everything looks good", ui::OK.green());
        return Ok(());
    }

    for finding in &findings {
        let glyph = match finding.level {
            Level::Error => ui::KO.red(),
            Level::Warning => "!".yellow().bold(),
            Level::Hint => "→".blue(),
        };
        println!("{glyph} {}", finding.title);
        if let Some(detail) = &finding.detail {
            println!("  {}", detail.dimmed());
        }
        if let Some(fix) = &finding.fix {
            println!("  {} {}", "fix:".dimmed(), ui::command(fix));
        }
    }

    let count = |level| findings.iter().filter(|f| f.level == level).count();
    println!();
    let summary: Vec<String> = [
        (Level::Error, "error"),
        (Level::Warning, "warning"),
        (Level::Hint, "hint"),
    ]
    .into_iter()
    .filter(|(level, _)| count(*level) > 0)
    .map(|(level, word)| ui::plural(count(level), word))
    .collect();
    println!("{}", summary.join(" · ").dimmed());
    if count(Level::Error) > 0 {
        return Err(GoatHookerError::HookFailed("doctor".into()));
    }
    Ok(())
}
