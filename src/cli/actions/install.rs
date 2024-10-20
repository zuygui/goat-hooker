use std::{error::Error, fs::{create_dir_all, File}, io::Write, path::PathBuf};

use crate::config::config::HookType;


pub fn install_hooks() -> Result<(), Box<dyn Error>> {
    let hooks_dir = PathBuf::from(".git/hooks");

    // Crée le répertoire des hooks si nécessaire
    create_dir_all(&hooks_dir)?;

    let hooks = vec![
        HookType::PreCommit,
        HookType::PrepareCommitMsg,
        HookType::CommitMsg,
        HookType::PostCommit,
        HookType::PreRebase,
        HookType::PostRewrite,
        HookType::PrePush,
        HookType::PreApplyPatch,
        HookType::PostApplyPatch,
        HookType::PreAutoGc,
        HookType::PostCheckout,
        HookType::PostMerge,
    ];

    for hook in hooks {
        let hook_name = get_hook_name(&hook);
        let hook_path = hooks_dir.join(&hook_name);

        // Ouvre ou crée le fichier de hook
        let mut file = File::create(hook_path)?;

        // Écrit la commande `hooker run <hook-name>` dans le fichier sans utiliser writeln!
        file.write_all(b"#!/bin/sh\n")?;
        let hook_command = format!("hooker run {}\n", hook_name);
        file.write_all(hook_command.as_bytes())?;
    }

    Ok(())
}

fn get_hook_name(hook_type: &HookType) -> &'static str {
    match hook_type {
        HookType::PreCommit => "pre-commit",
        HookType::PrepareCommitMsg => "prepare-commit-msg",
        HookType::CommitMsg => "commit-msg",
        HookType::PostCommit => "post-commit",
        HookType::PreRebase => "pre-rebase",
        HookType::PostRewrite => "post-rebase",
        HookType::PrePush => "pre-push",
        HookType::PreApplyPatch => "pre-applypatch",
        HookType::PostApplyPatch => "post-applypatch",
        HookType::PreAutoGc => "pre-auto-gc",
        HookType::PostCheckout => "post-checkout",
        HookType::PostMerge => "post-merge",
    }
}