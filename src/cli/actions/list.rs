use std::path::PathBuf;

use colored::Colorize;

use goat_hooker::{config::ConfigLoader, errors::GoatHookerError, git};

use super::run::resolve_config;

/// Shows each config and its hooks
pub fn list_hooks(config_path: Option<PathBuf>) -> Result<(), GoatHookerError> {
    let cwd = std::env::current_dir()?;
    let root = git::repo_root(&cwd)?;
    let Some(config_path) = resolve_config(&root, config_path) else {
        return Err(GoatHookerError::ConfigNotFound(
            root.join(goat_hooker::config::HOOK_CONFIG_FILENAME),
        ));
    };

    let configs = ConfigLoader::new().load_recursive(&config_path)?;

    for config in &configs {
        let file = config.file.clone().unwrap_or_default();
        let file = file.strip_prefix(&root).unwrap_or(&file).to_path_buf();
        println!("{}", format!("📁 {}", file.display()).bold().blue());

        let hooks = config.hooks.clone().unwrap_or_default();
        if hooks.is_empty() {
            println!("   {}", "no hooks".dimmed());
        }
        for hook in hooks {
            let trigger = hook.trigger.map(|t| t.as_str()).unwrap_or("-");
            println!("   🐐 {} {}", hook.name, format!("[{trigger}]").yellow());
            if let Some(include) = hook.include.as_ref() {
                println!("      include: {}", include.join(", ").dimmed());
            }
            if let Some(exclude) = hook.exclude.as_ref() {
                println!("      exclude: {}", exclude.join(", ").dimmed());
            }
            if let Some(needs) = hook.needs.as_ref() {
                println!("      needs: {}", needs.join(", ").dimmed());
            }
            for (name, source) in hook.inputs.iter().flatten() {
                println!(
                    "      input {}: {}",
                    format!("{{{name}}}").cyan(),
                    source.dimmed()
                );
            }
            if let Some(reads) = hook.reads.as_ref() {
                println!("      reads: {}", reads.join(", ").dimmed());
            }
            if let Some(writes) = hook.writes.as_ref() {
                println!("      writes: {}", writes.join(", ").dimmed());
            }
            if let Some(origin) = hook.origin.as_ref() {
                println!(
                    "      from: {}",
                    format!("{} ({})", origin.path, origin.url).dimmed()
                );
            }
            if hook.permissions.is_some() || hook.origin.is_some() {
                let granted = hook.granted();
                println!(
                    "      permissions: {}",
                    format!(
                        "files {}, {}, {}",
                        match granted.files {
                            goat_hooker::config::FileAccess::Read => "read",
                            goat_hooker::config::FileAccess::Write => "write",
                        },
                        if granted.network {
                            "network"
                        } else {
                            "no network"
                        },
                        match &granted.env {
                            None => "whole env".to_string(),
                            Some(env) if env.is_empty() => "basic env".to_string(),
                            Some(env) => format!("env {}", env.join(" ")),
                        }
                    )
                    .dimmed()
                );
            }
            if let Some(outputs) = hook.outputs.as_ref() {
                println!("      outputs: {}", outputs.join(", ").dimmed());
            }
            match hook.steps.as_ref() {
                Some(steps) => {
                    for step in steps {
                        println!("      › {}", step.id.bold());
                        if let Some(needs) = step.needs.as_ref() {
                            println!("          needs: {}", needs.join(", ").dimmed());
                        }
                        for (name, source) in step.inputs.iter().flatten() {
                            println!(
                                "          input {}: {}",
                                format!("{{{name}}}").cyan(),
                                source.dimmed()
                            );
                        }
                        println!("          🚧 {}", step.cmd);
                    }
                }
                None => {
                    for cmd in hook.commands() {
                        println!("      🚧 {cmd}");
                    }
                }
            }
        }
    }

    Ok(())
}
