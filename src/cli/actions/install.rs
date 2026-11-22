use colored::Colorize;

use goat_hooker::{errors::GoatHookerError, git, install};

pub fn install_hooks(force: bool) -> Result<(), GoatHookerError> {
    let cwd = std::env::current_dir()?;
    let root = git::repo_root(&cwd)?;

    let skipped = install::install_hooks(&root, force)?;
    for hook_path in &skipped {
        eprintln!(
            "{} kept {} {}",
            "!".yellow().bold(),
            hook_path.display(),
            "(not installed by goat-hooker, use --force to replace it)".dimmed()
        );
    }

    let hooks_dir = git::hooks_dir(&root)?;
    let hooks_dir = hooks_dir.strip_prefix(&root).unwrap_or(&hooks_dir);
    println!(
        "{} Git hooks installed in {}",
        "✔".green(),
        hooks_dir.display()
    );
    Ok(())
}
