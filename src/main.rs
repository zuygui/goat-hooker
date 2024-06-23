
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct CLI {
    #[command(subcommand)]
    cmd: Commands
}

#[derive(Subcommand, Debug, Clone)]
enum Commands {
    #[command(about = "Generate configuration files for the tool")]
    Init,
    #[command(about = "Generate hook linked script")]
    Add {
        name: String,
        event: String // TODO: create an enum with the list of possible git hooks events
    },
    #[command(about = "Link a subscript to an existing hook linked script")]
    Link {
        name: String,
        config: String // The Add->name property
    },
    #[command(about = "Run a hook (Use for debug)")]
    Run {
        config: String
    }
}

fn main() {
    let cli = CLI::parse();

    match cli.cmd {
        Commands::Init => (),
        Commands::Add { name, event } => (),
        Commands::Link { name, config } => (),
        Commands::Run { config: String } => ()
    }
}