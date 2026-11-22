use std::process;

use clap::Parser;
use colored::Colorize;

mod cli;

fn main() {
    let opt = cli::AppCli::parse();

    if let Err(err) = cli::parse_commands(opt) {
        eprintln!("{}", format!("❌ {err}").red());
        process::exit(1);
    }
}
