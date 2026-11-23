//! Cache of the hooks that passed: a hook isn't run again when its files (their content),
//! its commands, its inputs and the variables it reads didn't change. Its outputs, variables
//! and output are replayed.
//!
//! By default, only the hooks targeting files (`include`) are cached; `"cache": true` caches
//! another hook, `"cache": false` never caches it. Stored in `.git/goat-hooker/cache.json`.

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{errors::GoatHookerError, git, graph::Node};

/// Most entries kept (the oldest ones are dropped)
const MAX_ENTRIES: usize = 2000;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entry {
    /// Seconds since the epoch
    pub at: u64,
    pub outputs: BTreeMap<String, String>,
    pub variables: BTreeMap<String, String>,
    pub stdout: String,
}

#[derive(Debug)]
pub struct Cache {
    file: PathBuf,
    entries: Mutex<BTreeMap<String, Entry>>,
    changed: std::sync::atomic::AtomicBool,
}

fn file(root: &Path) -> Result<PathBuf, GoatHookerError> {
    Ok(git::data_dir(root)?.join("cache.json"))
}

impl Cache {
    pub fn load(root: &Path) -> Result<Self, GoatHookerError> {
        let file = file(root)?;
        let entries = fs::read_to_string(&file)
            .ok()
            .and_then(|c| serde_json::from_str(&c).ok())
            .unwrap_or_default();
        Ok(Self {
            file,
            entries: Mutex::new(entries),
            changed: Default::default(),
        })
    }

    pub fn get(&self, key: &str) -> Option<Entry> {
        self.entries.lock().unwrap().get(key).cloned()
    }

    pub fn put(&self, key: String, mut entry: Entry) {
        entry.at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or_default();
        self.entries.lock().unwrap().insert(key, entry);
        self.changed
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn save(&self) -> Result<(), GoatHookerError> {
        if !self.changed.load(std::sync::atomic::Ordering::Relaxed) {
            return Ok(());
        }
        let mut entries = self.entries.lock().unwrap().clone();
        if entries.len() > MAX_ENTRIES {
            let mut ages: Vec<u64> = entries.values().map(|e| e.at).collect();
            ages.sort_unstable();
            let oldest_kept = ages[ages.len() - MAX_ENTRIES];
            entries.retain(|_, e| e.at >= oldest_kept);
        }
        fs::create_dir_all(self.file.parent().unwrap())?;
        fs::write(
            &self.file,
            serde_json::to_string(&entries).unwrap_or_default(),
        )?;
        Ok(())
    }
}

/// Forgets every cached result
pub fn clear(root: &Path) -> Result<(), GoatHookerError> {
    let file = file(root)?;
    if file.exists() {
        fs::remove_file(file)?;
    }
    Ok(())
}

/// Is this node cached by default?
pub fn cacheable(node: &Node) -> bool {
    match node.hook.cache {
        Some(cache) => cache,
        None => node.hook.include.is_some() && !node.files.is_empty(),
    }
}

/// Key of a run of a node: changes with anything that can change its result
pub fn key(
    node: &Node,
    inputs: &[(String, String)],
    variables: &[(String, String)],
    args: &[String],
) -> Result<String, GoatHookerError> {
    let mut hasher = Sha256::new();
    let mut add = |part: &[u8]| {
        hasher.update((part.len() as u64).to_le_bytes());
        hasher.update(part);
    };
    add(env!("CARGO_PKG_VERSION").as_bytes());
    add(node.base_dir.to_string_lossy().as_bytes());
    add(node.reference().as_bytes());
    for cmd in node.commands() {
        add(cmd.as_bytes());
    }
    add(format!("{:?}", node.hook.granted()).as_bytes());
    for file in &node.files {
        add(file.to_string_lossy().as_bytes());
        // A missing file (deleted) is part of the key too
        add(&fs::read(node.base_dir.join(file)).unwrap_or_else(|_| b"\0deleted".to_vec()));
    }
    for (name, value) in inputs.iter().chain(variables) {
        add(name.as_bytes());
        add(value.as_bytes());
    }
    for arg in args {
        add(arg.as_bytes());
    }
    Ok(hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}
