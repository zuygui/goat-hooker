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

/// Trusts hooks files in their current version (kept, to show what changes next time)
pub fn trust(root: &Path, origins: &[Origin]) -> Result<(), GoatHookerError> {
    let mut trusted = load(root);
    let snapshots = git::data_dir(root)?.join("trusted");
    fs::create_dir_all(&snapshots)?;
    for origin in origins {
        trusted.insert(origin.key(), origin.hash.clone());
        if let Ok(content) = fs::read_to_string(&origin.file) {
            let snapshot = serde_json::json!({ "content": content, "with": origin.with });
            fs::write(
                snapshots.join(format!("{}.json", origin.hash)),
                snapshot.to_string(),
            )?;
        }
    }
    save(root, &trusted)
}

/// Hooks of a version of a hooks file, as they run (parameters filled, remote permissions)
fn hooks_of(content: &str, with: &str, origin: &Origin) -> Option<Vec<Hook>> {
    let with: Option<std::collections::BTreeMap<String, serde_json::Value>> =
        serde_json::from_str(with).ok().flatten();
    let tmp = std::env::temp_dir().join(format!(
        "goat-hooker-trust-{}-{}.json",
        std::process::id(),
        origin.hash
    ));
    fs::write(&tmp, content).ok()?;
    let config = crate::config::Config::load_with(&tmp, with.as_ref(), "");
    let _ = fs::remove_file(&tmp);
    Some(
        config
            .ok()?
            .hooks
            .unwrap_or_default()
            .into_iter()
            .map(|mut h| {
                h.origin = Some(origin.clone());
                h
            })
            .collect(),
    )
}

/// What changed since the trusted version of a hooks file (`None`: never trusted)
pub fn changes(root: &Path, origin: &Origin) -> Option<Vec<String>> {
    let previous_hash = load(root).get(&origin.key())?.clone();
    let snapshot: serde_json::Value = git::data_dir(root)
        .ok()
        .and_then(|d| {
            fs::read_to_string(d.join("trusted").join(format!("{previous_hash}.json"))).ok()
        })
        .and_then(|c| serde_json::from_str(&c).ok())?;
    // A hook of the pre-commit ecosystem: its definition in the .pre-commit-hooks.yaml
    if let Some((_, id)) = origin.path.split_once('#') {
        let definition = |content: &str| -> Option<String> {
            let hooks: serde_json::Value = serde_yaml_ng::from_str(content).ok()?;
            hooks
                .as_array()?
                .iter()
                .find(|h| h["id"].as_str() == Some(id))
                .map(|h| serde_json::to_string(h).unwrap_or_default())
        };
        let before = definition(snapshot["content"].as_str()?);
        let after = definition(&fs::read_to_string(&origin.file).ok()?);
        return Some(if before == after {
            vec![format!(
                "~ `{id}`: its arguments, dependencies or command in the config changed"
            )]
        } else {
            vec![format!(
                "~ `{id}` changed in {}: now {}",
                crate::tools::MANIFEST,
                after.unwrap_or_else(|| "removed".into())
            )]
        });
    }
    let before = hooks_of(
        snapshot["content"].as_str()?,
        snapshot["with"].as_str().unwrap_or("null"),
        origin,
    )?;
    let after = hooks_of(
        &fs::read_to_string(&origin.file).ok()?,
        &origin.with,
        origin,
    )?;

    let mut changes = Vec::new();
    for hook in &after {
        let Some(old) = before.iter().find(|h| h.name == hook.name) else {
            changes.push(format!(
                "+ new hook `{}`: {}",
                hook.name,
                hook.commands().join(" && ")
            ));
            continue;
        };
        if old.commands() != hook.commands() {
            changes.push(format!(
                "~ `{}` runs `{}` (was `{}`)",
                hook.name,
                hook.commands().join(" && "),
                old.commands().join(" && ")
            ));
        }
        if old.trigger != hook.trigger {
            changes.push(format!(
                "~ `{}` runs on {} (was {})",
                hook.name,
                hook.trigger.map(|t| t.as_str()).unwrap_or("-"),
                old.trigger.map(|t| t.as_str()).unwrap_or("-")
            ));
        }
        let (was, now) = (old.granted(), hook.granted());
        if was.files != now.files {
            changes.push(format!(
                "! `{}` may now {} files",
                hook.name,
                match now.files {
                    crate::config::FileAccess::Write => "modify",
                    crate::config::FileAccess::Read => "only read",
                }
            ));
        }
        if was.network != now.network {
            changes.push(format!(
                "! `{}` {} the network",
                hook.name,
                if now.network {
                    "now uses"
                } else {
                    "no longer uses"
                }
            ));
        }
        if was.env != now.env {
            changes.push(format!(
                "! `{}` environment: {}",
                hook.name,
                match &now.env {
                    None => "the whole environment".to_string(),
                    Some(env) if env.is_empty() => "basic".to_string(),
                    Some(env) => env.join(", "),
                }
            ));
        }
        if old.writes != hook.writes || old.reads != hook.reads {
            changes.push(format!("~ `{}` uses other variables", hook.name));
        }
        if old.include != hook.include || old.exclude != hook.exclude {
            changes.push(format!("~ `{}` targets other files", hook.name));
        }
    }
    for hook in &before {
        if !after.iter().any(|h| h.name == hook.name) {
            changes.push(format!("- removed hook `{}`", hook.name));
        }
    }
    if changes.is_empty() {
        changes.push("same hooks (their file or its parameters changed)".to_string());
    }
    Some(changes)
}

/// Stops trusting a hooks file
pub fn revoke(root: &Path, origin: &Origin) -> Result<(), GoatHookerError> {
    let mut trusted = load(root);
    if trusted.remove(&origin.key()).is_some() {
        save(root, &trusted)?;
    }
    Ok(())
}
