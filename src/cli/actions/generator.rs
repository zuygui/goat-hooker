use std::io;

use clap::{Command, CommandFactory};
use clap_complete::{generate, Generator, Shell};

use crate::cli::AppCli;

pub(crate) fn generate_completion(generator: Shell) {
    let mut cmd = AppCli::command();
    eprintln!("Generating completion file for {generator:?}...");
    print_completions(generator, &mut cmd);
}

fn print_completions<G: Generator>(gen: G, cmd: &mut Command) {
    generate(gen, cmd, cmd.get_name().to_string(), &mut io::stdout());
}
