use std::{thread, time::Duration};

use colored::Colorize;

use goat_hooker::{
    config::{ConfigLoader, HOOK_CONFIG_FILENAME},
    errors::GoatHookerError,
    watch::{self, Round},
};

use crate::cli::ui;

/// How often the index is checked
const POLL: Duration = Duration::from_millis(400);
/// The index must stay the same this long before the hooks run (several `git add`)
const SETTLE: Duration = Duration::from_millis(800);
/// Lines of output shown for a failed hook
const OUTPUT_LINES: usize = 30;

/// `hooker watch`: runs the cached pre-commit hooks each time files are staged
pub fn watch() -> Result<(), GoatHookerError> {
    let root = ui::current_repo()?;
    let config_path = root.join(HOOK_CONFIG_FILENAME);
    let load = || -> Result<_, GoatHookerError> {
        let configs = ConfigLoader::new().load_recursive(&config_path)?;
        Ok(watch::prepare(&configs))
    };

    let prepared = load()?;
    let names: Vec<String> = prepared
        .iter()
        .flat_map(|c| c.hooks.iter().flatten().map(|h| h.name.clone()))
        .collect();
    if names.is_empty() {
        println!(
            "{} No pre-commit hook can run ahead: they need `include`, and no `stage_fixed`, `needs`, `inputs` or variables",
            ui::PENDING.dimmed()
        );
        return Ok(());
    }
    println!(
        "{} {} {}",
        "👀 Watching the staged files".bold(),
        format!("· {}:", ui::plural(names.len(), "hook")).dimmed(),
        names.join(", ")
    );
    println!(
        "{}",
        "   Each `git add` runs them now, so the commit takes their results from the cache. Ctrl+C to stop.".dimmed()
    );

    let mut seen = None;
    loop {
        let state = watch::state(&root);
        if state != seen {
            // Wait until the staging is done
            thread::sleep(SETTLE);
            if watch::state(&root) != state {
                continue;
            }
            seen = state;
            // The configs may have changed too
            match load() {
                Ok(prepared) => round(&root, &prepared),
                Err(err) => println!("{} {err}", ui::KO.red()),
            }
        }
        thread::sleep(POLL);
    }
}

fn round(root: &std::path::Path, prepared: &[goat_hooker::config::Config]) {
    let results = match watch::run(root, prepared) {
        Ok(Round::Ran(results)) => results,
        Ok(Round::Nothing) | Ok(Round::Busy) => return,
        Ok(Round::Unstaged(files)) => {
            let names: Vec<String> = files
                .iter()
                .map(|f| {
                    f.strip_prefix(root)
                        .unwrap_or(f)
                        .to_string_lossy()
                        .to_string()
                })
                .collect();
            println!(
                "{} {} {}",
                ui::PENDING.dimmed(),
                "waiting: unstaged changes in".dimmed(),
                names.join(", ")
            );
            return;
        }
        Err(err) => {
            println!("{} {err}", ui::KO.red());
            return;
        }
    };
    if results.is_empty() {
        return;
    }

    let parts: Vec<String> = results
        .iter()
        .map(|r| {
            let status = if r.cached {
                "cached".dimmed().to_string()
            } else {
                ui::duration(r.duration).dimmed().to_string()
            };
            if r.success {
                format!("{} {} {status}", ui::OK.green(), r.name)
            } else {
                format!("{} {} {status}", ui::KO.red(), r.name)
            }
        })
        .collect();
    let failed: Vec<_> = results.iter().filter(|r| !r.success).collect();
    let verdict = if failed.is_empty() {
        "ready to commit".green().to_string()
    } else {
        "the commit would fail".red().to_string()
    };
    println!("{}  {}", parts.join("  "), verdict);

    for result in failed {
        let output = result.stdout();
        let lines: Vec<&str> = output.lines().collect();
        let start = lines.len().saturating_sub(OUTPUT_LINES);
        println!("{}", format!("── {} ──", result.name).red());
        for line in &lines[start..] {
            println!("  {line}");
        }
    }
}
