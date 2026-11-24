use std::collections::BTreeSet;

use colored::Colorize;

use goat_hooker::{
    config::{ConfigLoader, Trigger},
    errors::GoatHookerError,
    git, install,
    matcher::Matcher,
};

use crate::cli::ui::{self, PENDING};

/// `hooker` / `hooker status`: state of the repository at a glance
pub fn show_status() -> Result<(), GoatHookerError> {
    let root = ui::current_repo()?;
    let repo_name = root
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();

    println!(
        "{} {}",
        format!("🐐 {repo_name}").bold(),
        git::current_branch(&root)
            .map(|b| format!("on {b}"))
            .unwrap_or_default()
            .dimmed()
    );
    println!();

    // Git hooks
    if install::is_installed(&root) {
        println!(
            "  {:<11} {} {}",
            "Git hooks",
            "●".green(),
            "installed".green()
        );
    } else {
        println!(
            "  {:<11} {} not installed  {} {}",
            "Git hooks",
            "○".yellow(),
            "→ run".dimmed(),
            ui::command("hooker install")
        );
    }

    // Config
    let config_path = goat_hooker::config::config_path(&root);
    if !config_path.exists() {
        println!(
            "  {:<11} {} no hooker.toml  {} {}",
            "Config",
            "○".yellow(),
            "→ run".dimmed(),
            ui::command("hooker init")
        );
        return Ok(());
    }

    let configs = ConfigLoader::new().load_recursive(&config_path)?;
    let hooks: usize = configs
        .iter()
        .map(|c| c.hooks.iter().flatten().count())
        .sum();
    let parallel = configs.first().and_then(|c| c.parallel).unwrap_or(false);
    println!(
        "  {:<11} {} {}",
        "Config",
        config_path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy(),
        format!(
            "· {} · {}{}",
            ui::plural(configs.len(), "project"),
            ui::plural(hooks, "hook"),
            if parallel { " · parallel" } else { "" }
        )
        .dimmed()
    );

    // Projects
    println!();
    println!("  {}", "Projects".bold());
    let labels: Vec<String> = configs
        .iter()
        .map(|c| ui::project_label(&root, c.base_dir.as_deref().unwrap_or(&root)))
        .collect();
    let width = labels.iter().map(|l| l.chars().count()).max().unwrap_or(0);
    for (config, label) in configs.iter().zip(&labels) {
        let hooks: Vec<_> = config.hooks.iter().flatten().collect();
        let triggers: BTreeSet<&str> = hooks
            .iter()
            .filter_map(|h| h.trigger.map(|t| t.as_str()))
            .collect();
        let triggers: Vec<String> = Trigger::ALL
            .iter()
            .filter(|t| triggers.contains(t.as_str()))
            .map(|t| ui::trigger(*t).to_string())
            .collect();
        println!(
            "    {label:<width$}  {}  {}",
            format!("{:<8}", ui::plural(hooks.len(), "hook")).dimmed(),
            triggers.join(", ")
        );
    }

    // Next commit
    let staged = git::staged_files(&root)?;
    println!();
    println!(
        "  {} {}",
        "Next commit".bold(),
        format!("· {} staged", ui::plural(staged.len(), "file")).dimmed()
    );
    if staged.is_empty() {
        println!("    {}", "nothing staged".dimmed());
        return Ok(());
    }

    let matcher = Matcher::new(&configs)?;
    let to_run = matcher.hooks_to_run(Trigger::PreCommit, Some(&staged));
    if to_run.is_empty() {
        println!(
            "    {}",
            "no pre-commit hook targets the staged files".dimmed()
        );
        return Ok(());
    }

    let width = to_run
        .iter()
        .map(|m| m.hook.name.chars().count())
        .max()
        .unwrap_or(0);
    let project_width = to_run
        .iter()
        .map(|m| ui::project_label(&root, &m.base_dir).chars().count())
        .max()
        .unwrap_or(0);
    for matched in &to_run {
        let files: Vec<String> = matched
            .files
            .iter()
            .map(|f| f.display().to_string())
            .collect();
        let files = if files.len() > 3 {
            format!("{} +{}", files[..3].join(", "), files.len() - 3)
        } else {
            files.join(", ")
        };
        println!(
            "    {} {:<width$}  {}  {}",
            PENDING.dimmed(),
            matched.hook.name,
            format!(
                "{:<project_width$}",
                ui::project_label(&root, &matched.base_dir)
            )
            .dimmed(),
            files.dimmed()
        );
    }
    println!();
    println!(
        "  {} {}",
        "→ try them now with".dimmed(),
        ui::command("hooker run pre-commit")
    );

    Ok(())
}
