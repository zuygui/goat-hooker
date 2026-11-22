//! Checks if a newer version of goat-hooker was released (GitHub releases, through curl)

use std::process::Command;

const LATEST: &str = "https://api.github.com/repos/zuygui/goat-hooker/releases/latest";
pub const RELEASES: &str = "https://github.com/zuygui/goat-hooker/releases/latest";

/// `0.10.2` → `[0, 10, 2]`
fn parse(version: &str) -> Vec<u64> {
    version
        .trim_start_matches('v')
        .split(['.', '-'])
        .map_while(|p| p.parse().ok())
        .collect()
}

/// Is `latest` newer than `current`?
pub fn is_newer(latest: &str, current: &str) -> bool {
    parse(latest) > parse(current)
}

/// Last released version, if it is newer than this one
pub fn newer_version() -> Option<String> {
    let output = Command::new("curl")
        .args([
            "-fsSL",
            "--max-time",
            "10",
            "-H",
            "User-Agent: goat-hooker",
            LATEST,
        ])
        .output()
        .ok()?;
    let release: serde_json::Value = serde_json::from_slice(&output.stdout).ok()?;
    let tag = release["tag_name"].as_str()?;
    is_newer(tag, env!("CARGO_PKG_VERSION")).then(|| tag.trim_start_matches('v').to_string())
}

#[cfg(test)]
mod tests {
    use super::is_newer;

    #[test]
    fn versions() {
        assert!(is_newer("v0.3.0", "0.2.0"));
        assert!(is_newer("0.10.0", "0.9.9"));
        assert!(!is_newer("v0.2.0", "0.2.0"));
        assert!(!is_newer("0.1.9", "0.2.0"));
    }
}
