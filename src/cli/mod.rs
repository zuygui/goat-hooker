mod actions;
mod commands;

pub use commands::AppCli;
use commands::AppCommands;

pub fn parse_commands(commands: &AppCli) {
    match commands.cmd {
        AppCommands::Init => actions::init::init_configuration(),
        AppCommands::Install => {}
        AppCommands::Completion { shell } => actions::generator::generate_completion(shell),
    };
}
