use std::{path::PathBuf, process::Command, time::Instant};

use colored::Colorize;

use goat_hooker::{
    config::{ConfigLoader, Trigger},
    errors::GoatHookerError,
    runner::{self, HookInput, HookResult, Output, Progress},
    trust,
};

use super::run::{Reporter, print_outputs, print_summary};
use crate::cli::ui;

fn git(root: &std::path::Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
        .env("HOOKER_SKIP", "1")
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Commit to compare with: `--base`, the base of the pull request, or the default branch
fn base(root: &std::path::Path, base: Option<String>) -> Option<String> {
    let candidates: Vec<String> =
        match base.filter(|b| !b.is_empty() && !b.chars().all(|c| c == '0')) {
            Some(base) => vec![base],
            None => {
                let mut candidates = Vec::new();
                if let Ok(branch) = std::env::var("GITHUB_BASE_REF")
                    && !branch.is_empty()
                {
                    candidates.push(format!("origin/{branch}"));
                }
                candidates.push("origin/HEAD".to_string());
                candidates.push("HEAD~1".to_string());
                candidates
            }
        };
    for candidate in candidates {
        let exists = git(
            root,
            &[
                "rev-parse",
                "--verify",
                "-q",
                &format!("{candidate}^{{commit}}"),
            ],
        )
        .is_some();
        // A shallow clone may not have it yet
        if !exists && !candidate.starts_with("HEAD") {
            let remote_ref = candidate.trim_start_matches("origin/");
            let _ = git(
                root,
                &["fetch", "--no-tags", "--depth=100", "origin", remote_ref],
            );
        }
        if git(
            root,
            &[
                "rev-parse",
                "--verify",
                "-q",
                &format!("{candidate}^{{commit}}"),
            ],
        )
        .is_some()
        {
            return Some(candidate);
        }
    }
    None
}

/// Runs the hooks of a trigger on the files changed since a base commit (pull request, push)
pub fn run_ci(
    trigger: Trigger,
    base_ref: Option<String>,
    verbose: bool,
) -> Result<(), GoatHookerError> {
    let root = ui::current_repo()?;
    let config_path = goat_hooker::config::config_path(&root);
    if !goat_hooker::config::has_config(&root) {
        return Err(GoatHookerError::ConfigNotFound(config_path));
    }
    // The CI runs the code of the repository anyway: hooks of other repositories are trusted
    // SAFETY: set before any thread is started
    unsafe { std::env::set_var(trust::TRUST_ALL_ENV, "1") };
    let configs = ConfigLoader::new().load_recursive(&config_path)?;

    let Some(base) = base(&root, base_ref) else {
        return Err(GoatHookerError::GitError(
            "no base commit to compare with, use --base (and fetch-depth: 0 in actions/checkout)"
                .to_string(),
        ));
    };
    // Changes of the branch only (since the merge base), or since the base commit
    let from = git(&root, &["merge-base", &base, "HEAD"]).unwrap_or(base.clone());
    let changed = git(
        &root,
        &[
            "diff",
            "--name-only",
            "--no-renames",
            "--diff-filter=ACMR",
            "-z",
            &from,
            "HEAD",
        ],
    )
    .unwrap_or_default();
    let files: Vec<PathBuf> = changed
        .split('\0')
        .filter(|f| !f.is_empty())
        .map(|f| root.join(f))
        .collect();
    eprintln!(
        "{} {} changed since {}",
        "🐐".bold(),
        ui::plural(files.len(), "file"),
        base.cyan()
    );

    let input = HookInput {
        no_cache: true,
        ci_files: Some(files),
        ..Default::default()
    };
    let start = Instant::now();
    let reporter = Reporter::new(trigger, &root);
    let results = {
        let on_progress = |progress: Progress| reporter.on_progress(progress);
        runner::run_trigger(
            &root,
            &configs,
            trigger,
            &input,
            Output::Capture,
            Some(&on_progress),
        )?
    };
    reporter.finish();

    if results.is_empty() {
        eprintln!(
            "{}",
            format!("🐐 No {} hooks to run", trigger.as_str()).dimmed()
        );
        return Ok(());
    }
    print_outputs(&results, &root, verbose);
    print_summary(&results, start.elapsed());
    annotate(&results, &root);

    let failed: Vec<&str> = results
        .iter()
        .filter(|r| r.failed())
        .map(|r| r.name.as_str())
        .collect();
    if !failed.is_empty() {
        return Err(GoatHookerError::HookFailed(failed.join("`, `")));
    }
    Ok(())
}

/// GitHub Actions annotations of the failed hooks
fn annotate(results: &[HookResult], root: &std::path::Path) {
    if std::env::var("GITHUB_ACTIONS").as_deref() != Ok("true") {
        return;
    }
    for result in results.iter().filter(|r| r.failed()) {
        let message: String = result
            .stdout()
            .lines()
            .take(20)
            .collect::<Vec<_>>()
            .join("%0A");
        println!(
            "::error title=goat-hooker: {} ({})::{message}",
            result.name,
            ui::project_label(root, &result.base_dir)
        );
    }
}
