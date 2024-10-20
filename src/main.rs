extern crate colored;

use std::error::Error;

use clap::Parser;

mod cli;
mod config;

fn main() -> Result<(), Box<dyn Error>> {
    let opt = cli::AppCli::parse();

    cli::parse_commands(&opt)?;

    Ok(())
}
