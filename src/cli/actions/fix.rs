use std::{path::PathBuf, time::Instant};

use colored::Colorize;

use goat_hooker::{
    config::{ConfigLoader, HOOK_CONFIG_FILENAME, Trigger},
    errors::GoatHookerError,
    fix,
    runner::Progress,
};

use crate::cli::{
    actions::run::{Reporter, print_outputs, print_summary},
    ui,
};

/// `hooker fix`: runs the fixers on the changed files (or the given ones), without staging
pub fn fix_files(files: Vec<PathBuf>, verbose: bool) -> Result<(), GoatHookerError> {
    let root = ui::current_repo()?;
    let configs = ConfigLoader::new().load_recursive(&root.join(HOOK_CONFIG_FILENAME))?;
    let cwd = std::env::current_dir()?;
    let files = (!files.is_empty()).then(|| files.into_iter().map(|f| cwd.join(f)).collect());

    let start = Instant::now();
    let reporter = Reporter::new(Trigger::PreCommit, &root);
    let fixed = {
        let on_progress = |progress: Progress| reporter.on_progress(progress);
        fix::run(&root, &configs, files, Some(&on_progress))?
    };
    reporter.finish();

    if fixed.results.is_empty() {
        eprintln!(
            "{}",
            "🐐 Nothing to fix: no `stage_fixed` hook targets the changed files".dimmed()
        );
        return Ok(());
    }
    print_outputs(&fixed.results, &root, verbose);
    print_summary(&fixed.results, start.elapsed());

    if fixed.changed.is_empty() {
        eprintln!("{}", "   Nothing changed".dimmed());
    } else {
        eprintln!(
            "   {} {}",
            format!("Fixed {}:", ui::plural(fixed.changed.len(), "file")).green(),
            fixed
                .changed
                .iter()
                .map(|f| f.strip_prefix(&root).unwrap_or(f).display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        );
        eprintln!("   {}", "Not staged: check them, then `git add`".dimmed());
    }
    let failed: Vec<&str> = fixed
        .results
        .iter()
        .filter(|r| r.failed())
        .map(|r| r.name.as_str())
        .collect();
    if !failed.is_empty() {
        return Err(GoatHookerError::HookFailed(failed.join("`, `")));
    }
    Ok(())
}
