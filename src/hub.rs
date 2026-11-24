//! Finds hooks shared on GitHub: repositories whose root config (`hooker.toml` or
//! `hooker.config.json`) has `share`.
//!
//! GitHub's code search needs a token: `$GITHUB_TOKEN`, `$GH_TOKEN` or the one of the GitHub
//! CLI (`gh auth token`). Without it, only the hooks of goat-hooker itself are listed.
//! Requests go through `curl`, like Git, so the proxy and certificates of the system are used.

use std::{
    io::Write,
    process::{Command, Stdio},
};

use serde_json::Value;

use crate::{
    config::{Config, Param, Share, Trigger},
    errors::GoatHookerError,
};

const API: &str = "https://api.github.com";

/// Repository always listed (its hooks catalog)
pub const FEATURED: &str = "zuygui/goat-hooker";

/// More repositories to read, separated by commas (`owner/name` or `owner/name@branch`)
pub const EXTRA_ENV: &str = "HOOKER_HUB_REPOS";

/// Most repositories read for one search
const MAX_REPOSITORIES: usize = 30;
/// Most hooks files read in one repository
const MAX_FILES: usize = 20;

/// A repository sharing hooks
#[derive(Debug, Clone)]
pub struct SharedRepo {
    /// `owner/name`
    pub name: String,
    /// URL to use in `from`
    pub url: String,
    /// Branch read (`owner/name@branch`), used as `rev`
    pub rev: Option<String>,
    pub description: Option<String>,
    pub tags: Vec<String>,
    pub files: Vec<SharedFile>,
}

/// A shared hooks file
#[derive(Debug, Clone)]
pub struct SharedFile {
    /// Path in the repository, to use in `path`
    pub path: String,
    pub description: Option<String>,
    pub params: Vec<(String, Param)>,
    pub hooks: Vec<SharedHook>,
}

#[derive(Debug, Clone)]
pub struct SharedHook {
    pub name: String,
    pub trigger: Option<Trigger>,
    pub commands: Vec<String>,
    /// The hook as loaded from another repository (for its permissions)
    pub hook: crate::config::Hook,
}

impl SharedFile {
    /// Name of the hook loading this file: `rust` for `hooks/rust.json`
    pub fn default_name(&self) -> String {
        std::path::Path::new(&self.path)
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "shared".to_string())
    }
}

fn network_error(message: impl Into<String>) -> GoatHookerError {
    GoatHookerError::Network(message.into())
}

/// Token for the GitHub API
pub fn token() -> Option<String> {
    for var in ["GITHUB_TOKEN", "GH_TOKEN"] {
        if let Ok(token) = std::env::var(var)
            && !token.trim().is_empty()
        {
            return Some(token.trim().to_string());
        }
    }
    let output = Command::new("gh")
        .args(["auth", "token"])
        .stderr(Stdio::null())
        .output()
        .ok()?;
    let token = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (output.status.success() && !token.is_empty()).then_some(token)
}

/// GET with curl; the token goes through stdin, not the command line
fn get(url: &str, token: Option<&str>, raw: bool) -> Result<String, GoatHookerError> {
    let mut config = String::new();
    config.push_str("header = \"X-GitHub-Api-Version: 2022-11-28\"\n");
    config.push_str("header = \"User-Agent: goat-hooker\"\n");
    config.push_str(if raw {
        "header = \"Accept: application/vnd.github.raw\"\n"
    } else {
        "header = \"Accept: application/vnd.github+json\"\n"
    });
    if let Some(token) = token {
        config.push_str(&format!("header = \"Authorization: Bearer {token}\"\n"));
    }

    let mut child = Command::new("curl")
        .args([
            "-sSL",
            "--max-time",
            "20",
            "-w",
            "\n%{http_code}",
            "-K",
            "-",
            url,
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| network_error(format!("failed to run curl: {e}")))?;
    child
        .stdin
        .take()
        .unwrap()
        .write_all(config.as_bytes())
        .map_err(|e| network_error(e.to_string()))?;
    let output = child
        .wait_with_output()
        .map_err(|e| network_error(e.to_string()))?;
    if !output.status.success() {
        return Err(network_error(format!(
            "can't reach GitHub: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }

    let body = String::from_utf8_lossy(&output.stdout).to_string();
    let (body, status) = body.rsplit_once('\n').unwrap_or((&body, "0"));
    match status.trim() {
        "200" => Ok(body.to_string()),
        "401" => Err(network_error("GitHub refused the token (401)")),
        "403" | "429" => Err(network_error(
            "GitHub rate limit reached, try again in a minute",
        )),
        "404" => Err(network_error(format!("not found: {url}"))),
        status => Err(network_error(format!("GitHub answered {status} for {url}"))),
    }
}

fn get_json(url: &str, token: Option<&str>) -> Result<Value, GoatHookerError> {
    let body = get(url, token, false)?;
    serde_json::from_str(&body).map_err(|e| network_error(format!("invalid answer of GitHub: {e}")))
}

/// `%`-encoding of a query
fn encode(text: &str) -> String {
    text.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            b' ' => "+".to_string(),
            b => format!("%{b:02X}"),
        })
        .collect()
}

/// Repositories sharing hooks whose name, description, tags, files or hooks match `query`
/// (every word must match). Returns the repositories and a note (no token, partial results...).
pub fn search(query: &str) -> Result<(Vec<SharedRepo>, Option<String>), GoatHookerError> {
    let token = token();
    let mut names: Vec<String> = vec![FEATURED.to_string()];
    // Repositories code search can't find (private, forks, branches): `owner/name[@branch]`
    if let Ok(extra) = std::env::var(EXTRA_ENV) {
        names.extend(
            extra
                .split([',', ' '])
                .filter(|n| !n.is_empty())
                .map(str::to_string),
        );
    }
    let mut note = None;

    match token.as_deref() {
        Some(token) => {
            for file in crate::config::CONFIG_FILENAMES {
                let url = format!(
                    "{API}/search/code?per_page=100&q={}",
                    encode(&format!("share filename:{file} path:/"))
                );
                let found = get_json(&url, Some(token))?;
                for item in found["items"].as_array().into_iter().flatten() {
                    let path = item["path"].as_str().unwrap_or_default();
                    let Some(name) = item["repository"]["full_name"].as_str() else {
                        continue;
                    };
                    if path == file && !names.iter().any(|n| n == name) {
                        names.push(name.to_string());
                    }
                }
            }
        }
        None => {
            note = Some(
                "GitHub's code search needs a token (GITHUB_TOKEN, or `gh auth login`): only the hooks of goat-hooker are listed"
                    .to_string(),
            );
        }
    }
    names.truncate(MAX_REPOSITORIES);

    // One thread per repository
    let repos: Vec<Result<Option<SharedRepo>, GoatHookerError>> = std::thread::scope(|scope| {
        let handles: Vec<_> = names
            .iter()
            .map(|name| scope.spawn(|| read_repo(name, token.as_deref())))
            .collect();
        handles
            .into_iter()
            .map(|h| h.join().unwrap_or_else(|_| Ok(None)))
            .collect()
    });

    let mut found = Vec::new();
    let mut failed = 0;
    for repo in repos {
        match repo {
            Ok(Some(repo)) => found.push(repo),
            Ok(None) => {}
            Err(_) => failed += 1,
        }
    }
    if failed > 0 && note.is_none() {
        note = Some(format!("{failed} repositories couldn't be read"));
    }

    let words: Vec<String> = query.split_whitespace().map(|w| w.to_lowercase()).collect();
    let found = found
        .into_iter()
        .filter_map(|mut repo| {
            if words.is_empty() {
                return Some(repo);
            }
            let matches = |text: &str| words.iter().all(|w| text.contains(w));
            let repo_text = format!(
                "{} {} {}",
                repo.name,
                repo.description.clone().unwrap_or_default(),
                repo.tags.join(" ")
            )
            .to_lowercase();
            let all_files = repo.files.clone();
            repo.files.retain(|file| {
                let text = format!(
                    "{} {} {} {}",
                    repo.name,
                    file.path,
                    file.description.clone().unwrap_or_default(),
                    file.hooks
                        .iter()
                        .map(|h| h.name.as_str())
                        .collect::<Vec<_>>()
                        .join(" ")
                )
                .to_lowercase();
                matches(&text)
            });
            // Nothing in the files, but the repository itself matches (its tags)
            if repo.files.is_empty() && matches(&repo_text) {
                repo.files = all_files;
            }
            (!repo.files.is_empty()).then_some(repo)
        })
        .collect();
    Ok((found, note))
}

/// Reads the `share` of a repository and its hooks files (`None` if it doesn't share hooks)
/// `name` can end with `@branch` to read another branch than the default one.
pub fn read_repo(name: &str, token: Option<&str>) -> Result<Option<SharedRepo>, GoatHookerError> {
    let (name, rev) = match name.split_once('@') {
        Some((name, rev)) => (name, Some(rev)),
        None => (name, None),
    };
    let query = rev
        .map(|r| format!("?ref={}", encode(r)))
        .unwrap_or_default();
    // `hooker.toml` first, then `hooker.config.json`
    let mut root = None;
    for file in crate::config::CONFIG_FILENAMES {
        let url = format!("{API}/repos/{name}/contents/{file}{query}");
        if let Ok(content) = get(&url, token, true)
            && let Ok(value) = crate::config::read_value(std::path::Path::new(file), &content)
        {
            root = Some(value);
            break;
        }
    }
    let Some(root) = root else {
        return Ok(None);
    };
    let Some(share) = root
        .get("share")
        .and_then(|s| serde_json::from_value::<Share>(s.clone()).ok())
    else {
        return Ok(None);
    };

    // Files matching the `hooks` globs
    let tree = get_json(
        &format!(
            "{API}/repos/{name}/git/trees/{}?recursive=1",
            encode(rev.unwrap_or("HEAD"))
        ),
        token,
    )?;
    let patterns: Vec<glob::Pattern> = share
        .hooks
        .iter()
        .filter_map(|p| glob::Pattern::new(p.trim_start_matches("./")).ok())
        .collect();
    let paths: Vec<String> = tree["tree"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|e| e["type"] == "blob")
        .filter_map(|e| e["path"].as_str())
        .filter(|p| patterns.iter().any(|pattern| pattern.matches(p)))
        .take(MAX_FILES)
        .map(str::to_string)
        .collect();

    let mut files = Vec::new();
    for path in paths {
        let Ok(content) = get(
            &format!("{API}/repos/{name}/contents/{path}{query}"),
            token,
            true,
        ) else {
            continue;
        };
        // Read as written: the `{{params}}` stay in the commands
        let Some(config) = crate::config::read_value(std::path::Path::new(&path), &content)
            .ok()
            .and_then(|value| serde_json::from_value::<Config>(value).ok())
        else {
            continue;
        };
        files.push(SharedFile {
            path,
            description: config.description,
            params: config.params.unwrap_or_default().into_iter().collect(),
            hooks: config
                .hooks
                .unwrap_or_default()
                .iter()
                .map(|h| SharedHook {
                    name: h.name.clone(),
                    trigger: h.trigger,
                    commands: h.commands().iter().map(|c| c.to_string()).collect(),
                    hook: crate::config::Hook {
                        origin: Some(crate::config::Origin {
                            url: format!("https://github.com/{name}"),
                            rev: rev.map(str::to_string),
                            path: String::new(),
                            hash: String::new(),
                            file: Default::default(),
                            with: String::new(),
                            commit: None,
                        }),
                        ..h.clone()
                    },
                })
                .collect(),
        });
    }

    Ok(Some(SharedRepo {
        name: name.to_string(),
        url: format!("https://github.com/{name}"),
        rev: rev.map(str::to_string),
        description: share.description,
        tags: share.tags.unwrap_or_default(),
        files,
    }))
}

/// Adds a hook loading a shared file to a config, then trusts it (the user picked it)
pub fn add(
    root: &std::path::Path,
    config_file: &std::path::Path,
    name: &str,
    repo: &SharedRepo,
    file: &SharedFile,
    with: std::collections::BTreeMap<String, Value>,
) -> Result<(), GoatHookerError> {
    let mut config = Config::load_from_file(config_file)?;
    let hooks = config.hooks.get_or_insert_with(Vec::new);
    if hooks.iter().any(|h| h.name == name) {
        return Err(GoatHookerError::InvalidHook {
            name: name.to_string(),
            reason: "this project already has a hook with this name".to_string(),
        });
    }
    let hook = crate::config::Hook {
        name: name.to_string(),
        from: Some(repo.url.clone()),
        rev: repo.rev.clone(),
        path: Some(file.path.clone()),
        with: (!with.is_empty()).then_some(with),
        ..Default::default()
    };
    hook.validate()?;
    hooks.push(hook);
    config.write_to_file(config_file)?;

    // Fetches it, and trusts the version that was reviewed
    let configs =
        crate::config::ConfigLoader::new().load_recursive(&crate::config::config_path(root))?;
    let origins: Vec<crate::config::Origin> =
        crate::trust::untrusted(root, configs.iter().flat_map(|c| c.hooks.iter().flatten()))
            .into_iter()
            .filter(|o| o.url == repo.url && o.path == file.path)
            .collect();
    crate::trust::trust(root, &origins)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encoding() {
        assert_eq!(
            encode("share filename:hooker.config.json path:/"),
            "share+filename%3Ahooker.config.json+path%3A%2F"
        );
    }
}
