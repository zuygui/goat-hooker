mod actions;
mod commands;

pub use commands::AppCli;
use commands::AppCommands;

use crate::errors::GoatHookerError;

pub fn parse_commands(commands: AppCli) -> Result<(), GoatHookerError> {
    match commands.cmd {
        AppCommands::Init { work_dir } => actions::init::init_configuration(&work_dir),
        AppCommands::Install { force } => actions::install::install_hooks(force),
        AppCommands::Completion { shell } => {
            actions::generator::print_completion(shell);
            Ok(())
        }
        AppCommands::Run {
            hook_type,
            config_path,
            args,
        } => actions::run::run_hooks(hook_type, config_path, &args),
    }
}
