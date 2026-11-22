use colored::Colorize;

use goat_hooker::{errors::GoatHookerError, git, install};

pub fn install_hooks(force: bool) -> Result<(), GoatHookerError> {
    let cwd = std::env::current_dir()?;
    let root = git::repo_root(&cwd)?;

    for hook_path in install::install_hooks(&root, force)? {
        eprintln!(
            "{}",
            format!(
                "⚠️ Warning: {} already exists and wasn't installed by goat-hooker. Skipping (use --force to overwrite).",
                hook_path.display()
            )
            .yellow()
        );
    }

    println!("{}", "✅ Hooks installed successfully".green());
    Ok(())
}
