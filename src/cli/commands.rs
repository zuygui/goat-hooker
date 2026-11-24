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

        /// Hooks to start with: rust, javascript, python, checks, commits, or `auto` (from the
        /// files of the project). Asked in a terminal when missing
        #[arg(short = 't', long, value_delimiter = ',')]
        template: Vec<String>,
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

        /// Config file (default: `hooker.toml` at the repository root)
        #[arg(short = 'c', value_parser, value_hint = clap::ValueHint::FilePath)]
        config_path: Option<PathBuf>,

        /// Show the hooks that would run (and their files) without running them
        #[arg(long)]
        dry_run: bool,

        /// Show the output of every hook, not only of the failed ones
        #[arg(short, long)]
        verbose: bool,

        /// Run every hook, even the ones that already passed with the same files
        #[arg(long)]
        no_cache: bool,

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
        /// Config file (default: `hooker.toml` at the repository root)
        #[arg(short = 'c', value_parser, value_hint = clap::ValueHint::FilePath)]
        config_path: Option<PathBuf>,
    },

    /// Update the hooks loaded from git repositories (`from`)
    #[command()]
    Update {
        /// Config file (default: `hooker.toml` at the repository root)
        #[arg(short = 'c', value_parser, value_hint = clap::ValueHint::FilePath)]
        config_path: Option<PathBuf>,
    },

    /// Convert the configs of husky / lint-staged, lefthook and pre-commit to hooker.toml
    #[command()]
    Import {
        /// Replace the existing hooker configs
        #[arg(short, long)]
        force: bool,

        /// Show the converted hooks without writing them
        #[arg(long)]
        dry_run: bool,
    },

    /// Search hooks shared on GitHub and add them
    #[command()]
    Search {
        /// Words to look for (rust, eslint, commits...); empty lists everything
        query: Vec<String>,

        /// Project folder to add the hooks to (default: the root)
        #[arg(short, long, value_hint = clap::ValueHint::DirPath)]
        project: Option<PathBuf>,
    },

    /// Show the hooks loaded from other repositories that have a newer version
    #[command()]
    Outdated,

    /// Pin the hooks loaded from other repositories to the commit in use
    #[command()]
    Pin {
        /// Only this hook (the one loading the hooks file)
        name: Option<String>,
    },

    /// Review the hooks loaded from other repositories (`from`) and trust them
    #[command()]
    Trust {
        /// Trust all of them without asking
        #[arg(long)]
        all: bool,
    },

    /// List the variables shared between hooks: value, writers, readers
    #[command()]
    Vars {
        /// Forget the value of a variable (back to its default)
        #[arg(long, value_name = "NAME")]
        reset: Option<String>,
    },

    /// Run the hooks on the files changed by a pull request or a push (CI)
    #[command()]
    Ci {
        /// Git hook whose hooks run
        #[arg(short, long, value_enum, default_value = "pre-commit")]
        trigger: Trigger,

        /// Commit or branch to compare with (default: the base of the pull request, or origin/HEAD)
        #[arg(long)]
        base: Option<String>,

        /// Show the output of every hook
        #[arg(short, long)]
        verbose: bool,
    },

    /// Run the hooks fixing files (`stage_fixed`) on the changed files, without staging them
    #[command()]
    Fix {
        /// Files to fix (default: the files changed since the last commit)
        #[arg(value_hint = clap::ValueHint::FilePath)]
        files: Vec<PathBuf>,

        /// Show the output of every hook
        #[arg(short, long)]
        verbose: bool,
    },

    /// Run the cached pre-commit hooks as soon as files are staged, so that commits are instant
    #[command()]
    Watch,

    /// Check the setup: Git hooks, missing tools, patterns matching nothing, variables, trust
    #[command()]
    Doctor,

    /// Show the last runs of the hooks and the slowest hooks
    #[command()]
    History {
        /// Number of runs (default: 20, every run with `--export`)
        #[arg(short = 'n', long)]
        limit: Option<usize>,

        /// Totals by hook for the team: `csv` (one line per hook) or `json` (totals and runs)
        #[arg(long, value_parser = ["csv", "json"])]
        export: Option<String>,

        /// File to write the export to (default: stdout)
        #[arg(short = 'o', long, requires = "export")]
        output: Option<PathBuf>,
    },

    /// Forget the results of the hooks that passed (they all run again)
    #[command(name = "cache-clear")]
    CacheClear,

    /// Print the JSON schema of the configuration file
    #[command()]
    Schema,

    /// Look for a regex in files (the `pygrep` hooks of the pre-commit ecosystem): fails when
    /// a line matches
    #[command(hide = true)]
    Pygrep {
        #[arg(short = 'i', long)]
        ignore_case: bool,
        /// The regex may match several lines
        #[arg(long)]
        multiline: bool,
        /// Fails when a file has no match instead
        #[arg(long)]
        negate: bool,
        pattern: String,
        files: Vec<PathBuf>,
    },
}

fn styles() -> Styles {
    Styles::styled()
        .header(AnsiColor::Green.on_default() | Effects::BOLD)
        .usage(AnsiColor::Blue.on_default() | Effects::BOLD)
        .literal(AnsiColor::White.on_default() | Effects::BOLD)
        .placeholder(AnsiColor::BrightYellow.on_default())
}
