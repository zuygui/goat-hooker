use std::io::Error;
use std::path::PathBuf;
use std::process::{Command, Stdio};
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

pub fn run_hooks(p: PathBuf) -> Result<(), RunError> {
    let config: AppConfig = AppConfig::read(p).map_err(RunError::from)?;
    let mut results = Vec::new(); // Nouveau vecteur pour stocker les résultats

    let hooks = config.hooks;
    for hook in hooks {
        match hook {
            HookRule::Command(command) => match command {
                CommandHookRule::Single(data) => {
                    execute_command(data.clone())?;
                    results.push(data); // Ajouter le résultat au vecteur
                },
                CommandHookRule::Multiple(data) => {
                    for cmd in data {
                        execute_command(cmd.clone())?;
                        results.push(cmd); // Ajouter le résultat au vecteur
                    }
                },
            }
        }
    }

    Ok(())
}
