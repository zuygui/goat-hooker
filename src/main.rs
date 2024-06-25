use clap::Parser;

mod cli;
mod config;

fn main() {
    let opt = cli::AppCli::parse();

    cli::parse_commands(&opt);
}
