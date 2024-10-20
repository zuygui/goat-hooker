mod actions;
mod commands;

use std::{error::Error, process};

pub use commands::AppCli;
use commands::AppCommands;

use colored::Colorize;

pub fn parse_commands(commands: &AppCli) -> Result<(), Box<dyn Error>>{
    match commands.cmd.clone() {
        AppCommands::Init { work_dir } => {
            if let Err(err) = actions::init::init_configuration(work_dir) {
                println!("{}", format!("{}", err).red());
                process::exit(1);
            }
        }
        AppCommands::Install => actions::install::install_hooks()?,
        AppCommands::Completion { shell } => actions::generator::install_completion(shell)?,
        AppCommands::Run { config_path, hook_type } => {
            if let Err(err) = actions::run::run_hooks(config_path, hook_type) {
                println!("{}", format!("{}", err).red());
                process::exit(1);
            }
        }
    };

    Ok(())
}
