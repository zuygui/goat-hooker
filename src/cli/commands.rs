use clap::{
    builder::{
        styling::{AnsiColor, Effects},
        Styles,
    },
    Parser, Subcommand,
};
use clap_complete::Shell;

#[derive(Parser)]
#[command(author, version, about, long_about = None, styles = styles())]
pub struct AppCli {
    #[command(subcommand)]
    pub cmd: AppCommands,
}

#[derive(Subcommand, Debug, Clone)]
pub enum AppCommands {
    /// Initialize configuration file
    #[command()]
    Init {
        #[arg(short = 'w', value_hint = clap::ValueHint::DirPath)]
        work_dir: Option<std::path::PathBuf>,
    },

    /// Install Git Hooks
    #[command()]
    Install,

    /// Generate shell completion
    /// If provided, outputs the completion file for given shell
    #[command()]
    Completion {
        #[arg(default_value_t = Shell::Bash)]
        shell: Shell,
    },
}

fn styles() -> Styles {
    Styles::styled()
        .header(AnsiColor::Green.on_default() | Effects::BOLD)
        .usage(AnsiColor::Blue.on_default() | Effects::BOLD)
        .literal(AnsiColor::White.on_default() | Effects::BOLD)
        .placeholder(AnsiColor::BrightYellow.on_default())
}
