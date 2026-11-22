//! Mise en forme partagée par les commandes

use std::{
    io::IsTerminal,
    path::{Path, PathBuf},
    time::Duration,
};

use colored::{ColoredString, Colorize};

use goat_hooker::{config::Trigger, git};

pub const OK: &str = "✔";
pub const KO: &str = "✘";
pub const PENDING: &str = "·";

/// Le terminal est interactif (prompts, spinners)
pub fn is_interactive() -> bool {
    std::io::stdin().is_terminal() && std::io::stderr().is_terminal()
}

/// Nom d'un projet du monorepo : `root` ou son dossier relatif à la racine
pub fn project_label(root: &Path, dir: &Path) -> String {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    match dir.strip_prefix(&root) {
        Ok(rel) if rel.as_os_str().is_empty() => "root".to_string(),
        Ok(rel) => rel.display().to_string(),
        Err(_) => dir.display().to_string(),
    }
}

/// `120ms`, `1.23s`
pub fn duration(d: Duration) -> String {
    if d.as_millis() < 1000 {
        format!("{}ms", d.as_millis())
    } else {
        format!("{:.2}s", d.as_secs_f64())
    }
}

pub fn plural(n: usize, word: &str) -> String {
    format!("{n} {word}{}", if n > 1 { "s" } else { "" })
}

/// Couleur d'un trigger (la même que dans l'app)
pub fn trigger(trigger: Trigger) -> ColoredString {
    match trigger {
        Trigger::PreCommit => trigger.as_str().blue(),
        Trigger::PrePush => trigger.as_str().magenta(),
        Trigger::CommitMsg | Trigger::PrepareCommitMsg => trigger.as_str().cyan(),
        _ => trigger.as_str().yellow(),
    }
}

/// Une commande à lancer, pour les pistes de solution
pub fn command(cmd: &str) -> ColoredString {
    format!("`{cmd}`").bold()
}

/// Racine du dépôt qui contient le dossier courant
pub fn current_repo() -> Result<PathBuf, goat_hooker::errors::GoatHookerError> {
    let cwd = std::env::current_dir()?;
    git::repo_root(&cwd)
}
