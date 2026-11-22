use std::path::PathBuf;

use clap::{
    Parser, Subcommand,
    builder::{
        Styles,
        styling::{AnsiColor, Effects},
    },
};
use clap_complete::Shell;

use goat_hooker::config::Trigger;

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
        #[arg(short = 'w', default_value = ".", value_parser, value_hint = clap::ValueHint::DirPath)]
        work_dir: PathBuf,
    },

    /// Install Git Hooks
    #[command()]
    Install {
        /// Overwrite hooks that weren't installed by goat-hooker
        #[arg(short, long)]
        force: bool,
    },

    /// Generate shell completion
    /// Outputs the completion file for given shell on stdout
    #[command()]
    Completion {
        #[arg(default_value_t = Shell::Bash)]
        shell: Shell,
    },

    /// Run manually the hooks
    #[command()]
    Run {
        #[arg(value_enum)]
        hook_type: Trigger,

        /// Config file (default: `hooker.config.json` at the repository root)
        #[arg(short = 'c', value_parser, value_hint = clap::ValueHint::FilePath)]
        config_path: Option<PathBuf>,

        /// Show the hooks that would run (and their files) without running them
        #[arg(long)]
        dry_run: bool,

        /// Arguments given by Git to the hook, available as `$1`, `$2`... in commands
        #[arg(last = true)]
        args: Vec<String>,
    },

    /// List the configurations and their hooks
    #[command()]
    List {
        /// Config file (default: `hooker.config.json` at the repository root)
        #[arg(short = 'c', value_parser, value_hint = clap::ValueHint::FilePath)]
        config_path: Option<PathBuf>,
    },

    /// Update the hooks loaded from git repositories (`from`)
    #[command()]
    Update {
        /// Config file (default: `hooker.config.json` at the repository root)
        #[arg(short = 'c', value_parser, value_hint = clap::ValueHint::FilePath)]
        config_path: Option<PathBuf>,
    },

    /// Print the JSON schema of the configuration file
    #[command()]
    Schema,
}

fn styles() -> Styles {
    Styles::styled()
        .header(AnsiColor::Green.on_default() | Effects::BOLD)
        .usage(AnsiColor::Blue.on_default() | Effects::BOLD)
        .literal(AnsiColor::White.on_default() | Effects::BOLD)
        .placeholder(AnsiColor::BrightYellow.on_default())
}
