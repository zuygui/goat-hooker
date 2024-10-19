use std::io::Error;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use clap::ValueEnum;

use crate::config::{self, impls::ConfigError, rules::{CommandHookRule, HookRule}, AppConfig};

#[derive(thiserror::Error, Debug)]
pub enum RunError {
    #[error(transparent)]
    Config(#[from] ConfigError),

    #[error(transparent)]
    Io(#[from] Error)
}

fn execute_command(data: String) -> Result<(), RunError> {
    println!("🚧 Executing command: `{data}`");
    match Command::new("sh")
        .arg("-c")
        .arg(data)
        .stdout(Stdio::inherit()) // Rediriger la sortie standard vers le processus parent
        .stderr(Stdio::inherit()) // Rediriger la sortie d'erreur vers le processus parent
        .spawn()
    {
        Ok(mut child) => {
            let status = child.wait()?; // Attendre que le processus enfant se termine
            if status.success() {
                println!("✅ Command executed successfully");
            } else {
                eprintln!("❌ Command failed with exit code: {}", status);
            }
            Ok(())
        }
        Err(err) => {
            eprintln!("❌ An error occurred while executing command: {}", err);
            Err(RunError::Config(ConfigError::from(err)))
        }
    }
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum, Debug)]
pub enum HookTypeToRun {
    All,
    PreCommit,
    PrepareCommitMsg,
    CommitMsg,
    PostCommit,
    PreRebase,
    PostRewrite,
    PrePush,
    PreApplyPatch,
    PostApplyPatch,
    PreAutoGc,
    PostCheckout,
    PostMerge,
}

pub fn run_hooks(p: PathBuf, hooks_to_run: HookTypeToRun) -> Result<(), RunError> {
    let config: AppConfig = AppConfig::read(p).map_err(RunError::from)?;
    let mut results = Vec::new(); 

    // Fonction pour exécuter les règles de hook
    let mut execute_hook_rules = |h_rules: Vec<HookRule>| -> Result<(), RunError> {
        for rule in h_rules {
            match rule {
                HookRule::Command(command) => match command {
                    CommandHookRule::Single(data) => {
                        execute_command(data.clone())?;
                        results.push(data); 
                    }
                    CommandHookRule::Multiple(data) => {
                        for cmd in data {
                            execute_command(cmd.clone())?;
                            results.push(cmd); 
                        }
                    }
                }
            }
        }
        Ok(())
    };

    // Parcours de tous les hooks dans la configuration
    for (h_type, h_rules) in config.hooks {
        match hooks_to_run {
            HookTypeToRun::All => {
                // Exécuter toutes les règles pour tous les hooks
                execute_hook_rules(h_rules)?;
            }
            HookTypeToRun::PreCommit => {
                if let config::config::HookType::PreCommit = h_type {
                    execute_hook_rules(h_rules)?;
                }
            }
            HookTypeToRun::PrepareCommitMsg => {
                if let config::config::HookType::PrepareCommitMsg = h_type {
                    execute_hook_rules(h_rules)?;
                }
            }
            HookTypeToRun::CommitMsg => {
                if let config::config::HookType::CommitMsg = h_type {
                    execute_hook_rules(h_rules)?;
                }
            }
            HookTypeToRun::PostCommit => {
                if let config::config::HookType::PostCommit = h_type {
                    execute_hook_rules(h_rules)?;
                }
            }
            HookTypeToRun::PreRebase => {
                if let config::config::HookType::PreRebase = h_type {
                    execute_hook_rules(h_rules)?;
                }
            }
            HookTypeToRun::PostRewrite => {
                if let config::config::HookType::PostRewrite = h_type {
                    execute_hook_rules(h_rules)?;
                }
            }
            HookTypeToRun::PrePush => {
                if let config::config::HookType::PrePush = h_type {
                    execute_hook_rules(h_rules)?;
                }
            }
            HookTypeToRun::PreApplyPatch => {
                if let config::config::HookType::PreApplyPatch = h_type {
                    execute_hook_rules(h_rules)?;
                }
            }
            HookTypeToRun::PostApplyPatch => {
                if let config::config::HookType::PostApplyPatch = h_type {
                    execute_hook_rules(h_rules)?;
                }
            }
            HookTypeToRun::PreAutoGc => {
                if let config::config::HookType::PreAutoGc = h_type {
                    execute_hook_rules(h_rules)?;
                }
            }
            HookTypeToRun::PostCheckout => {
                if let config::config::HookType::PostCheckout = h_type {
                    execute_hook_rules(h_rules)?;
                }
            }
            HookTypeToRun::PostMerge => {
                if let config::config::HookType::PostMerge = h_type {
                    execute_hook_rules(h_rules)?;
                }
            }
        }
    }

    Ok(())
}
