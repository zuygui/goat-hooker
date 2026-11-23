//! Recently opened repositories (shown on the welcome page)

use std::{fs, path::PathBuf};

/// Number of repositories kept
const MAX_RECENTS: usize = 8;

/// Config folder of the app
/// (`$GOAT_HOOKER_CONFIG_DIR`, else the one of the OS: Application Support, AppData, ~/.config)
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

/// The recent repositories that still exist, from the newest to the oldest
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
    // Not a problem if it fails: the list will just be empty next time
    if let Some(dir) = file.parent() {
        let _ = fs::create_dir_all(dir);
    }
    if let Ok(content) = serde_json::to_string_pretty(recents) {
        let _ = fs::write(file, content);
    }
}

/// Puts the repository first in the list
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

/// Folders added with “Scan a folder…”
pub fn load_scan_roots() -> Vec<PathBuf> {
    let Some(content) = config_dir().and_then(|d| fs::read_to_string(d.join("scan.json")).ok())
    else {
        return vec![];
    };
    serde_json::from_str::<Vec<PathBuf>>(&content)
        .unwrap_or_default()
        .into_iter()
        .filter(|p| p.is_dir())
        .collect()
}

pub fn add_scan_root(dir: PathBuf) -> Vec<PathBuf> {
    let mut roots = load_scan_roots();
    if !roots.contains(&dir) {
        roots.push(dir);
    }
    if let Some(config) = config_dir() {
        let _ = fs::create_dir_all(&config);
        if let Ok(content) = serde_json::to_string_pretty(&roots) {
            let _ = fs::write(config.join("scan.json"), content);
        }
    }
    roots
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
        // SAFETY: only this test changes GOAT_HOOKER_CONFIG_DIR
        unsafe { std::env::set_var("GOAT_HOOKER_CONFIG_DIR", dir.path().join("config")) };

        for repo in &repos {
            add(repo.clone());
        }
        add(repos[3].clone());

        let recents = load();
        assert_eq!(recents.len(), MAX_RECENTS);
        assert_eq!(recents[0], repos[3]);
        assert_eq!(recents[1], repos[9]);

        // a repository deleted from the disk disappears from the list
        fs::remove_dir(&repos[9]).unwrap();
        assert!(!load().contains(&repos[9]));

        remove(&repos[3]);
        assert!(!load().contains(&repos[3]));
    }
}
