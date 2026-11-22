//! Dépôts ouverts récemment (affichés sur la page d'accueil)

use std::{fs, path::PathBuf};

/// Nombre de dépôts gardés
const MAX_RECENTS: usize = 8;

/// Dossier de config de l'app
/// (`$GOAT_HOOKER_CONFIG_DIR`, sinon celui de l'OS : Application Support, AppData, ~/.config)
fn config_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("GOAT_HOOKER_CONFIG_DIR") {
        return Some(PathBuf::from(dir));
    }
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let base = if cfg!(target_os = "macos") {
        home?.join("Library").join("Application Support")
    } else if cfg!(windows) {
        PathBuf::from(std::env::var_os("APPDATA")?)
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| home.map(|h| h.join(".config")))?
    };
    Some(base.join("goat-hooker"))
}

fn file() -> Option<PathBuf> {
    config_dir().map(|dir| dir.join("recent.json"))
}

/// Les dépôts récents qui existent encore, du plus récent au plus ancien
pub fn load() -> Vec<PathBuf> {
    let Some(content) = file().and_then(|f| fs::read_to_string(f).ok()) else {
        return vec![];
    };
    serde_json::from_str::<Vec<PathBuf>>(&content)
        .unwrap_or_default()
        .into_iter()
        .filter(|p| p.is_dir())
        .collect()
}

fn save(recents: &[PathBuf]) {
    let Some(file) = file() else {
        return;
    };
    // Pas grave si ça échoue : la liste sera juste vide au prochain lancement
    if let Some(dir) = file.parent() {
        let _ = fs::create_dir_all(dir);
    }
    if let Ok(content) = serde_json::to_string_pretty(recents) {
        let _ = fs::write(file, content);
    }
}

/// Met le dépôt en tête de liste
pub fn add(repo: PathBuf) -> Vec<PathBuf> {
    let mut recents = load();
    recents.retain(|p| *p != repo);
    recents.insert(0, repo);
    recents.truncate(MAX_RECENTS);
    save(&recents);
    recents
}

pub fn remove(repo: &PathBuf) -> Vec<PathBuf> {
    let mut recents = load();
    recents.retain(|p| p != repo);
    save(&recents);
    recents
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keep_recent_repositories() {
        let dir = tempfile::tempdir().unwrap();
        let repos: Vec<PathBuf> = (0..10)
            .map(|i| {
                let repo = dir.path().join(format!("repo{i}"));
                fs::create_dir(&repo).unwrap();
                repo
            })
            .collect();
        // SAFETY: seul ce test modifie GOAT_HOOKER_CONFIG_DIR
        unsafe { std::env::set_var("GOAT_HOOKER_CONFIG_DIR", dir.path().join("config")) };

        for repo in &repos {
            add(repo.clone());
        }
        add(repos[3].clone());

        let recents = load();
        assert_eq!(recents.len(), MAX_RECENTS);
        assert_eq!(recents[0], repos[3]);
        assert_eq!(recents[1], repos[9]);

        // un dépôt supprimé du disque disparaît de la liste
        fs::remove_dir(&repos[9]).unwrap();
        assert!(!load().contains(&repos[9]));

        remove(&repos[3]);
        assert!(!load().contains(&repos[3]));
    }
}
