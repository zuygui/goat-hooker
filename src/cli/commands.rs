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
    /// Without a command, shows the status of the repository
    #[command(subcommand)]
    pub cmd: Option<AppCommands>,
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

        /// Show the output of every hook, not only of the failed ones
        #[arg(short, long)]
        verbose: bool,

        /// Run by Git (stays quiet when there is nothing to run)
        #[arg(long, hide = true)]
        from_git: bool,

        /// Arguments given by Git to the hook, available as `$1`, `$2`... in commands
        #[arg(last = true)]
        args: Vec<String>,
    },

    /// Show the status of the repository: Git hooks, projects, next commit
    #[command()]
    Status,

    /// Add a hook (asks what's missing in a terminal)
    #[command()]
    Add {
        /// Hook name
        #[arg(short, long)]
        name: Option<String>,

        /// Git hook that runs it
        #[arg(short, long, value_enum)]
        trigger: Option<Trigger>,

        /// Command to run (repeat for several commands)
        #[arg(short, long = "cmd")]
        commands: Vec<String>,

        /// Files glob patterns, relative to the project (repeat or separate with commas)
        #[arg(short, long)]
        include: Vec<String>,

        /// Ignored files glob patterns
        #[arg(short, long)]
        exclude: Vec<String>,

        /// Re-stage the files after the hook (formatters)
        #[arg(long)]
        stage_fixed: bool,

        /// Project folder (default: the project of the current folder)
        #[arg(short, long, value_hint = clap::ValueHint::DirPath)]
        project: Option<PathBuf>,
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
