mod actions;
mod commands;

use std::process;

pub use commands::AppCli;
use commands::AppCommands;

use colored::Colorize;

pub fn parse_commands(commands: &AppCli) {
    match commands.cmd.clone() {
        AppCommands::Init { work_dir } => {
            if let Err(err) = actions::init::init_configuration(work_dir) {
                println!("{}", format!("{}", err).red());
                process::exit(1);
            }
        }
        AppCommands::Install => {}
        AppCommands::Completion { shell } => actions::generator::generate_completion(shell),
    };
}
