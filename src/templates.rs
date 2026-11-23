//! Hooks to start with (`hooker init`): the hooks files of the catalogue (`hooks/`),
//! built into the binary, and the ones matching the files of a project.

use std::path::Path;

use crate::{
    config::{Config, Hook},
    errors::GoatHookerError,
};

/// A hooks file of the catalogue
pub struct Template {
    pub name: &'static str,
    content: &'static str,
    /// Files showing that a project needs it
    markers: &'static [&'static str],
}

pub const TEMPLATES: &[Template] = &[
    Template {
        name: "rust",
        content: include_str!("../hooks/rust.json"),
        markers: &["Cargo.toml"],
    },
    Template {
        name: "javascript",
        content: include_str!("../hooks/javascript.json"),
        markers: &["package.json"],
    },
    Template {
        name: "python",
        content: include_str!("../hooks/python.json"),
        markers: &["pyproject.toml", "requirements.txt", "setup.py"],
    },
    Template {
        name: "checks",
        content: include_str!("../hooks/checks.json"),
        markers: &[],
    },
    Template {
        name: "commits",
        content: include_str!("../hooks/commits.json"),
        markers: &[],
    },
];

impl Template {
    pub fn find(name: &str) -> Option<&'static Template> {
        let name = name.trim().to_lowercase();
        let name = match name.as_str() {
            "js" | "ts" | "typescript" | "node" => "javascript",
            "py" => "python",
            "commit" | "conventional" => "commits",
            other => other,
        };
        TEMPLATES.iter().find(|t| t.name == name)
    }

    /// What the hooks file does
    pub fn description(&self) -> String {
        serde_json::from_str::<serde_json::Value>(self.content)
            .ok()
            .and_then(|v| v.get("description")?.as_str().map(String::from))
            .unwrap_or_default()
    }

    /// Its hooks, with the default values of its parameters
    pub fn hooks(&self) -> Result<Vec<Hook>, GoatHookerError> {
        let path = Path::new("hooks").join(format!("{}.json", self.name));
        let config = Config::parse_with(&path, self.content, None, self.name)?;
        Ok(config.hooks.unwrap_or_default())
    }
}

/// A new config with the hooks of these templates
pub fn config_with(templates: &[&Template]) -> Result<Config, GoatHookerError> {
    let mut hooks = Vec::new();
    for template in templates {
        hooks.extend(template.hooks()?);
    }
    Ok(Config {
        schema: Some(crate::config::SCHEMA_URL.to_string()),
        hooks: Some(hooks),
        ..Default::default()
    })
}

/// Templates matching the files of `dir` (`checks` is always there)
pub fn detect(dir: &Path) -> Vec<&'static Template> {
    TEMPLATES
        .iter()
        .filter(|t| t.name == "checks" || t.markers.iter().any(|marker| dir.join(marker).exists()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_template_loads() {
        for template in TEMPLATES {
            let hooks = template.hooks().unwrap();
            assert!(!hooks.is_empty(), "{}", template.name);
            for hook in hooks {
                hook.validate().unwrap();
                assert!(!hook.commands().join("").contains("{{"), "{}", hook.name);
            }
        }
    }

    #[test]
    fn templates_are_detected_from_the_files() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("Cargo.toml"), "").unwrap();
        let names: Vec<&str> = detect(dir.path()).iter().map(|t| t.name).collect();
        assert_eq!(names, ["rust", "checks"]);
        assert_eq!(Template::find("ts").unwrap().name, "javascript");
    }
}
