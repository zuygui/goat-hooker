use std::io;

use clap::CommandFactory;
use clap_complete::{Shell, generate};

use crate::cli::AppCli;

/// Writes the completion file to stdout
/// (ex: `hooker completion zsh > ~/.zfunc/_hooker`)
pub fn print_completion(shell: Shell) {
    let mut cmd = AppCli::command();
    generate(shell, &mut cmd, "hooker", &mut io::stdout());
}
