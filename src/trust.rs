//! Hooks loaded from other repositories (`from`) only run once trusted:
//! `hooker trust` (or the app) shows their commands and permissions and records their hash.
//! When the hooks file or the `with` values change, they must be trusted again.
//! `HOOKER_TRUST_ALL=1` skips the check (CI).

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use crate::{
    config::{Hook, Origin},
    errors::GoatHookerError,
    git,
};

pub const TRUST_ALL_ENV: &str = "HOOKER_TRUST_ALL";

fn store(root: &Path) -> Result<PathBuf, GoatHookerError> {
    Ok(git::data_dir(root)?.join("trusted.json"))
}

fn load(root: &Path) -> BTreeMap<String, String> {
    store(root)
        .ok()
        .and_then(|f| fs::read_to_string(f).ok())
        .and_then(|c| serde_json::from_str(&c).ok())
        .unwrap_or_default()
}

fn save(root: &Path, trusted: &BTreeMap<String, String>) -> Result<(), GoatHookerError> {
    let file = store(root)?;
    fs::create_dir_all(file.parent().unwrap())?;
    fs::write(
        file,
        serde_json::to_string_pretty(trusted).unwrap_or_default() + "\n",
    )?;
    Ok(())
}

/// Hooks files of other repositories that aren't trusted (or changed since), without duplicates
pub fn untrusted<'a>(root: &Path, hooks: impl IntoIterator<Item = &'a Hook>) -> Vec<Origin> {
    if std::env::var_os(TRUST_ALL_ENV).is_some_and(|v| v == "1") {
        return vec![];
    }
    let trusted = load(root);
    let mut origins: Vec<Origin> = Vec::new();
    for hook in hooks {
        let Some(origin) = hook.origin.as_ref() else {
            continue;
        };
        if trusted.get(&origin.key()) != Some(&origin.hash)
            && !origins.iter().any(|o| o.key() == origin.key())
        {
            origins.push(origin.clone());
        }
    }
    origins
}

/// Is this hooks file trusted in its current version?
pub fn is_trusted(root: &Path, origin: &Origin) -> bool {
    load(root).get(&origin.key()) == Some(&origin.hash)
}

/// Fails when one of the hooks isn't trusted
pub fn check<'a>(
    root: &Path,
    hooks: impl IntoIterator<Item = &'a Hook>,
) -> Result<(), GoatHookerError> {
    let untrusted = untrusted(root, hooks);
    if untrusted.is_empty() {
        return Ok(());
    }
    Err(GoatHookerError::Untrusted(
        untrusted
            .iter()
            .map(|o| format!("{} ({})", o.path, o.url))
            .collect(),
    ))
}

/// Trusts hooks files in their current version
pub fn trust(root: &Path, origins: &[Origin]) -> Result<(), GoatHookerError> {
    let mut trusted = load(root);
    for origin in origins {
        trusted.insert(origin.key(), origin.hash.clone());
    }
    save(root, &trusted)
}

/// Stops trusting a hooks file
pub fn revoke(root: &Path, origin: &Origin) -> Result<(), GoatHookerError> {
    let mut trusted = load(root);
    if trusted.remove(&origin.key()).is_some() {
        save(root, &trusted)?;
    }
    Ok(())
}
