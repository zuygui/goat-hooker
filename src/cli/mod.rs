mod actions;
mod commands;

pub use commands::AppCli;
use commands::AppCommands;

use goat_hooker::errors::GoatHookerError;

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
            dry_run,
            args,
        } => actions::run::run_hooks(hook_type, config_path, &args, dry_run),
        AppCommands::List { config_path } => actions::list::list_hooks(config_path),
        AppCommands::Update { config_path } => actions::update::update_remotes(config_path),
        AppCommands::Schema => {
            print!("{}", goat_hooker::config::json_schema());
            Ok(())
        }
    }
}
