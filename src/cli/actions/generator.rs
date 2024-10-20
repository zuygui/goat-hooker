use std::{error::Error, fs::{File, create_dir_all}, path::Path};

use clap::CommandFactory;
use clap_complete::{generate, Shell};

use crate::cli::AppCli;

pub(crate) fn install_completion(generator: Shell) -> Result<(), Box<dyn Error>> {
    let mut cmd = AppCli::command();
    let app_name = "hooker";
    eprintln!("👷 Generating completion file for {generator:?}...");

    // Définition du chemin de base et du format du nom de fichier
    let (base_dir, file_extension) = match generator {
        Shell::Bash => ("/etc/bash_completion.d", ""),
        Shell::Zsh => ("/usr/local/share/zsh/site-functions", "_"),
        Shell::Fish => ("~/.config/fish/completions", ".fish"),
        Shell::PowerShell => ("~/.config/powershell/completions", ".ps1"),
        Shell::Elvish => ("~/.config/elvish/lib/completions", ".elv"),
        _ => return Err("Unsupported shell".into()),
    };

    // Expansions pour les chemins contenant ~
    let base_dir = shellexpand::tilde(base_dir).into_owned();
    create_dir_all(Path::new(&base_dir))?;

    // Utilisation de format! pour générer le nom du fichier
    let file_path = format!("{}/{}{}", base_dir, app_name, file_extension);
    let mut file = File::create(Path::new(&file_path))?;

    // Génération des complétions
    generate(generator, &mut cmd, app_name, &mut file);

    eprintln!("✅ Successfully installed {:?} completion at {}", generator, file_path);
    
    Ok(())
}
