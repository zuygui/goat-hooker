use std::path::PathBuf;

use std::process::Command;

use crate::config::{self, impls::ConfigError, rules::{CommandHookRule, HookRule}, AppConfig};

#[derive(thiserror::Error, Debug)]
pub enum RunError {
    #[error(transparent)]
    Config(#[from] ConfigError),
}

fn execute_command(data: String) -> Result<(), RunError> {
    println!("🚧 Executing `{}`", data);
    match Command::new("sh")
        .arg("-c")
        .arg(data)
        .spawn() {
            Ok(_) => println!("✅ Command executed successfully"),
            Err(err) => eprintln!("❌ An error occurred while executing command: {}", err)
        }
    
    Ok(())
}

pub fn run_hooks(p: PathBuf) -> Result<(), RunError> {
    let config: AppConfig = AppConfig::read(p).map_err(RunError::from)?;

    let hooks = config.hooks;
    for hook in hooks {
        match hook {
            HookRule::Command(command) => match command {
              CommandHookRule::Single(data) => execute_command(data)?,
              CommandHookRule::Multiple(data) => for cmd in data { execute_command(cmd)? },
            }
          }
    }

    Ok(())
}
