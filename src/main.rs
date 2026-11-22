use std::process;

use clap::Parser;
use colored::Colorize;

mod cli;
mod config;
mod errors;
mod git;
mod matcher;

fn main() {
    let opt = cli::AppCli::parse();

    if let Err(err) = cli::parse_commands(opt) {
        eprintln!("{}", format!("❌ {err}").red());
        process::exit(1);
    }
}
