mod actions;
mod commands;
mod ui;

pub use commands::AppCli;
use commands::AppCommands;

use goat_hooker::errors::GoatHookerError;

pub fn parse_commands(commands: AppCli) -> Result<(), GoatHookerError> {
    let Some(cmd) = commands.cmd else {
        return actions::status::show_status();
    };
    match cmd {
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
            verbose,
            from_git,
            args,
        } => actions::run::run_hooks(
            hook_type,
            config_path,
            &args,
            actions::run::RunFlags {
                dry_run,
                verbose,
                from_git,
            },
        ),
        AppCommands::Status => actions::status::show_status(),
        AppCommands::Add {
            name,
            trigger,
            commands,
            include,
            exclude,
            stage_fixed,
            project,
        } => actions::add::add_hook(actions::add::AddArgs {
            name,
            trigger,
            commands,
            include,
            exclude,
            stage_fixed,
            project,
        }),
        AppCommands::List { config_path } => actions::list::list_hooks(config_path),
        AppCommands::Update { config_path } => actions::update::update_remotes(config_path),
        AppCommands::Import { force, dry_run } => actions::import::import_configs(force, dry_run),
        AppCommands::Trust { all } => actions::trust::trust_hooks(all),
        AppCommands::Vars { reset } => actions::vars::variables(reset),
        AppCommands::Schema => {
            print!("{}", goat_hooker::config::json_schema());
            Ok(())
        }
    }
}
