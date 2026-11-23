use colored::Colorize;

use goat_hooker::{
    errors::GoatHookerError,
    history::{self, Status},
};

use crate::cli::ui;

/// Shows the last runs and the slowest hooks
pub fn show_history(limit: usize) -> Result<(), GoatHookerError> {
    let root = ui::current_repo()?;
    let runs = history::load(&root, limit);
    if runs.is_empty() {
        println!("{} No runs yet", ui::PENDING.dimmed());
        return Ok(());
    }

    for run in &runs {
        let glyph = if run.passed() {
            ui::OK.green()
        } else {
            ui::KO.red()
        };
        let count = |status: Status| run.hooks.iter().filter(|h| h.status == status).count();
        let mut parts = vec![format!("{} passed", count(Status::Passed))];
        for (status, label) in [
            (Status::Cached, "cached"),
            (Status::Failed, "failed"),
            (Status::Denied, "denied"),
            (Status::Skipped, "skipped"),
        ] {
            if count(status.clone()) > 0 {
                parts.push(format!("{} {label}", count(status)));
            }
        }
        println!(
            "{} {:<12} {:<18} {:<14} {:>7}  {}",
            glyph,
            history::ago(run.at).dimmed(),
            goat_hooker::config::Trigger::from_name(&run.trigger)
                .map(ui::trigger)
                .unwrap_or_default(),
            run.branch.clone().unwrap_or_default().cyan(),
            ui::duration(std::time::Duration::from_millis(run.duration_ms)),
            parts.join(", ").dimmed()
        );
    }

    let slowest = history::slowest(&runs);
    if !slowest.is_empty() {
        println!("\n{}", "Slowest hooks".bold());
        for (project, name, average, count) in slowest.iter().take(5) {
            println!(
                "  {:>7}  {} {}",
                ui::duration(*average),
                name.bold(),
                format!("({project}, {} runs)", count).dimmed()
            );
        }
    }
    Ok(())
}
