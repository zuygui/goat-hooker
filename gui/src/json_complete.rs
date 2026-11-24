//! Autocompletion of a JSON config (`hooker.config.json`): where the cursor is (which key of which object),
//! and what can be written there (keys of the schema, triggers, hooks, variables, outputs...).

use goat_hooker::config::{BUILTIN_OUTPUTS, SCHEMA_URL, Trigger};

/// A step of the path to the cursor
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Segment {
    Key(String),
    Index,
}

/// Where the cursor is
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Position {
    /// Objects and arrays around the cursor, from the root
    pub path: Vec<Segment>,
    /// Writing a key (`true`) or a value
    pub key: bool,
    /// Text typed of the current key or value (without its opening quote)
    pub prefix: String,
    /// Byte offset where the current token starts (its quote included)
    pub start: usize,
    /// Keys already written in the current object
    pub siblings: Vec<String>,
    /// The value at the cursor is already written (after `"..."`, `]` or `}`)
    pub done: bool,
}

#[derive(Default)]
struct Frame {
    array: bool,
    /// Key being written or whose value is being written
    key: Option<String>,
    expecting_key: bool,
    keys: Vec<String>,
    /// A value (or an array item) was written since the last `:` or `,`
    value_done: bool,
}

/// Reads the text up to the cursor (it doesn't have to be valid JSON)
pub fn position(text: &str, offset: usize) -> Position {
    let text = &text[..offset.min(text.len())];
    let mut stack: Vec<Frame> = Vec::new();
    let mut string_start: Option<usize> = None;
    let mut escaped = false;
    let mut word_start: Option<usize> = None;

    for (i, c) in text.char_indices() {
        if let Some(start) = string_start {
            match (escaped, c) {
                (true, _) => escaped = false,
                (false, '\\') => escaped = true,
                (false, '"') => {
                    string_start = None;
                    if let Some(frame) = stack.last_mut() {
                        if !frame.array && frame.expecting_key {
                            frame.key = Some(text[start + 1..i].to_string());
                        } else {
                            frame.value_done = true;
                        }
                    }
                }
                _ => {}
            }
            continue;
        }
        match c {
            '"' => {
                string_start = Some(i);
                word_start = None;
            }
            '{' => {
                stack.push(Frame {
                    expecting_key: true,
                    ..Default::default()
                });
                word_start = None;
            }
            '[' => {
                stack.push(Frame {
                    array: true,
                    ..Default::default()
                });
                word_start = None;
            }
            '}' | ']' => {
                stack.pop();
                if let Some(parent) = stack.last_mut() {
                    parent.value_done = true;
                }
                word_start = None;
            }
            ':' => {
                if let Some(frame) = stack.last_mut() {
                    frame.expecting_key = false;
                    frame.value_done = false;
                    if let Some(key) = frame.key.clone() {
                        frame.keys.push(key);
                    }
                }
                word_start = None;
            }
            ',' => {
                if let Some(frame) = stack.last_mut() {
                    frame.value_done = false;
                    if !frame.array {
                        frame.expecting_key = true;
                        frame.key = None;
                    }
                }
                word_start = None;
            }
            c if c.is_ascii_alphanumeric() || c == '-' || c == '.' => {
                word_start.get_or_insert(i);
            }
            _ => word_start = None,
        }
    }

    let mut path = Vec::new();
    for frame in &stack {
        if frame.array {
            path.push(Segment::Index);
        } else if let Some(key) = frame.key.as_ref().filter(|_| !frame.expecting_key) {
            path.push(Segment::Key(key.clone()));
        }
    }
    // The last frame is the object or array the cursor is in: it isn't a step of the path
    let inside = stack.last();
    if let Some(frame) = inside
        && (frame.array || !frame.expecting_key)
    {
        path.pop();
    }

    let key = inside.is_some_and(|f| !f.array && f.expecting_key);
    let (prefix, start) = match (string_start, word_start) {
        (Some(start), _) => (text[start + 1..].to_string(), start),
        (None, Some(start)) => (text[start..].to_string(), start),
        (None, None) => (String::new(), text.len()),
    };
    // Value of a key: the key is the last step of the path
    if let Some(frame) = inside
        && !frame.array
        && !frame.expecting_key
        && let Some(k) = frame.key.clone()
    {
        path.push(Segment::Key(k));
    }

    Position {
        path,
        key,
        prefix,
        start,
        siblings: inside.map(|f| f.keys.clone()).unwrap_or_default(),
        done: inside.is_some_and(|f| f.value_done) && string_start.is_none(),
    }
}

/// What the config already holds, to suggest it
#[derive(Debug, Default, Clone)]
pub struct Known {
    /// Hooks of this project, and of the others (`project:hook`)
    pub hooks: Vec<String>,
    pub variables: Vec<String>,
    /// `hook.output` that can be an input
    pub outputs: Vec<String>,
}

/// A suggestion
#[derive(Debug, Clone, PartialEq)]
pub struct Suggestion {
    pub label: String,
    /// Text inserted in place of the current token
    pub insert: String,
    pub detail: String,
}

/// Keys of an object, with their description: (key, description, snippet of the value)
fn keys(path: &[Segment]) -> Vec<(&'static str, &'static str, &'static str)> {
    // (key, description, example of its value)
    let keys: Vec<&str> = path
        .iter()
        .filter_map(|s| match s {
            Segment::Key(k) => Some(k.as_str()),
            Segment::Index => None,
        })
        .collect();
    let last = keys.last().copied();
    let parent = keys.len().checked_sub(2).map(|i| keys[i]);
    match (parent, last) {
        (_, None) => vec![
            ("$schema", "JSON schema (autocompletion in editors)", "\"\""),
            ("hooks", "Hooks of this project", "[]"),
            ("imports", "Sub-projects folders (globs supported)", "[]"),
            (
                "depends_on",
                "Projects this one depends on, besides its manifests",
                "[]",
            ),
            (
                "discover",
                "Load every hooker config of the repository",
                "true",
            ),
            (
                "parallel",
                "Run the hooks in parallel (root config)",
                "true",
            ),
            ("variables", "Values shared between hooks", "{}"),
            ("params", "Parameters of a reusable hooks file", "{}"),
            ("description", "What this hooks file does", "\"\""),
            (
                "share",
                "Share hooks files of this repository on GitHub",
                "{}",
            ),
        ],
        (Some("hooks"), Some("steps")) | (_, Some("steps")) => vec![
            ("id", "Step id, unique in the hook", "\"\""),
            ("cmd", "Shell command of the step", "\"\""),
            ("needs", "Steps or hooks it waits for", "[]"),
            ("inputs", "Values from other steps or hooks", "{}"),
            ("outputs", "Outputs it writes to $HOOKER_OUTPUT", "[]"),
        ],
        (_, Some("hooks")) => vec![
            ("name", "Hook name", "\"\""),
            ("trigger", "Git hook that runs it", "\"pre-commit\""),
            ("cmd", "Shell command, {files} = targeted files", "\"\""),
            ("cmds", "Shell commands, run in order", "[]"),
            ("include", "Glob patterns of the targeted files", "[]"),
            ("exclude", "Glob patterns of the ignored files", "[]"),
            (
                "stage_fixed",
                "Re-stage the files it fixes (formatters)",
                "true",
            ),
            ("needs", "Hooks it waits for", "[]"),
            (
                "inputs",
                "Values from other hooks: { \"name\": \"hook.output\" }",
                "{}",
            ),
            ("outputs", "Outputs it writes to $HOOKER_OUTPUT", "[]"),
            ("steps", "Steps, each one can wait for the others", "[]"),
            ("reads", "Variables it reads ($name)", "[]"),
            ("writes", "Variables it writes ($HOOKER_VARS)", "[]"),
            ("permissions", "What it may do (files, network, env)", "{}"),
            (
                "cache",
                "Skip it when it passed with the same files",
                "true",
            ),
            ("branches", "Run it only on these branches (globs)", "[]"),
            (
                "skip_branches",
                "Don't run it on these branches (globs)",
                "[]",
            ),
            (
                "timeout",
                "Stop it and fail after this time (30s, 2m)",
                "\"\"",
            ),
            (
                "affected",
                "Also run it when a project this one depends on changes",
                "true",
            ),
            ("path", "Hooks file to load", "\"\""),
            ("from", "Git repository of the hooks file", "\"\""),
            ("rev", "Branch, tag or commit of `from`", "\"\""),
            ("with", "Values of the parameters of the hooks file", "{}"),
        ],
        (_, Some("permissions")) => vec![
            (
                "files",
                "read: can't modify the files, write: can",
                "\"read\"",
            ),
            ("network", "Network access (blocked when false)", "false"),
            ("env", "Environment variables it gets", "[]"),
        ],
        (Some("variables"), Some(_)) => vec![
            ("description", "What the variable holds", "\"\""),
            ("default", "Value before any hook writes it", "\"\""),
            ("writers", "Hooks allowed to write it", "[]"),
            ("readers", "Hooks allowed to read it", "[]"),
        ],
        (Some("params"), Some(_)) => vec![
            ("description", "What the parameter is for", "\"\""),
            ("default", "Value when `with` doesn't set it", "\"\""),
        ],
        (_, Some("share")) => vec![
            ("description", "What these hooks are for", "\"\""),
            ("tags", "Keywords to find them", "[]"),
            ("hooks", "Shared hooks files (globs supported)", "[]"),
        ],
        _ => vec![],
    }
}

/// Values for a key
fn values(path: &[Segment], known: &Known) -> Vec<(String, String)> {
    let mut keys = path.iter().rev().filter_map(|s| match s {
        Segment::Key(k) => Some(k.as_str()),
        Segment::Index => None,
    });
    let key = keys.next().unwrap_or_default();
    let parent = keys.next().unwrap_or_default();
    let quoted = |items: Vec<String>, detail: &str| -> Vec<(String, String)> {
        items
            .into_iter()
            .map(|i| (format!("\"{i}\""), detail.to_string()))
            .collect()
    };
    match key {
        "trigger" => quoted(
            Trigger::ALL
                .iter()
                .map(|t| t.as_str().to_string())
                .collect(),
            "Git hook",
        ),
        "needs" => quoted(known.hooks.clone(), "hook"),
        "reads" | "writes" => quoted(known.variables.clone(), "variable"),
        "writers" | "readers" => {
            let mut hooks = vec!["*".to_string()];
            hooks.extend(known.hooks.clone());
            quoted(hooks, "hook")
        }
        "files" if parent == "permissions" => quoted(vec!["read".into(), "write".into()], "access"),
        "$schema" => quoted(vec![SCHEMA_URL.to_string()], "schema"),
        "stage_fixed" | "parallel" | "discover" | "network" | "cache" | "affected" => vec![
            ("true".to_string(), "boolean".to_string()),
            ("false".to_string(), "boolean".to_string()),
        ],
        _ if parent == "inputs" => {
            let mut outputs = known.outputs.clone();
            for hook in &known.hooks {
                outputs.extend(BUILTIN_OUTPUTS.iter().map(|o| format!("{hook}.{o}")));
            }
            quoted(outputs, "output")
        }
        _ => vec![],
    }
}

/// Suggestions at the cursor
pub fn suggest(text: &str, offset: usize, known: &Known) -> (Position, Vec<Suggestion>) {
    let position = position(text, offset);
    let prefix = position.prefix.to_lowercase();
    let suggestions = if position.done && position.prefix.is_empty() {
        vec![]
    } else if position.key {
        keys(&position.path)
            .into_iter()
            .filter(|(k, _, _)| !position.siblings.iter().any(|s| s == k))
            .filter(|(k, _, _)| k.to_lowercase().starts_with(&prefix))
            .map(|(k, detail, value)| Suggestion {
                label: k.to_string(),
                // The value is typed next: `[`, `{` or `"` bring its own suggestions
                insert: format!("\"{k}\": "),
                detail: format!("{detail} · {value}"),
            })
            .collect()
    } else {
        values(&position.path, known)
            .into_iter()
            .filter(|(v, _)| v.trim_matches('"').to_lowercase().starts_with(&prefix))
            .map(|(v, detail)| Suggestion {
                label: v.trim_matches('"').to_string(),
                insert: v,
                detail,
            })
            .collect()
    };
    (position, suggestions)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(text: &str) -> (Position, Vec<Suggestion>) {
        let known = Known {
            hooks: vec!["lint".into(), "api:codegen".into()],
            variables: vec!["version".into()],
            outputs: vec!["api:codegen.schema".into()],
        };
        let offset = text.find('|').unwrap();
        suggest(&text.replace('|', ""), offset, &known)
    }

    fn labels(s: &[Suggestion]) -> Vec<&str> {
        s.iter().map(|s| s.label.as_str()).collect()
    }

    #[test]
    fn keys_of_a_hook() {
        let (position, s) = at(r#"{ "hooks": [ { "name": "x", "tr|" } ] }"#);
        assert_eq!(
            position.path,
            vec![Segment::Key("hooks".into()), Segment::Index]
        );
        assert!(position.key);
        assert_eq!(labels(&s), vec!["trigger"]);
        assert_eq!(s[0].insert, "\"trigger\": ");
    }

    #[test]
    fn written_keys_are_not_suggested() {
        let (_, s) = at(r#"{ "hooks": [ { "name": "x", | } ] }"#);
        assert!(!labels(&s).contains(&"name"));
        assert!(labels(&s).contains(&"cmd"));
    }

    #[test]
    fn values() {
        assert_eq!(
            labels(&at(r#"{ "hooks": [ { "trigger": "pre-p|" } ] }"#).1),
            vec!["pre-push"]
        );
        assert_eq!(
            labels(&at(r#"{ "hooks": [ { "needs": ["|"] } ] }"#).1),
            vec!["lint", "api:codegen"]
        );
        assert_eq!(
            labels(&at(r#"{ "hooks": [ { "reads": [ "v|" ] } ] }"#).1),
            vec!["version"]
        );
        assert_eq!(
            labels(&at(r#"{ "hooks": [ { "permissions": { "files": "|" } } ] }"#).1),
            vec!["read", "write"]
        );
        assert!(
            labels(&at(r#"{ "hooks": [ { "inputs": { "schema": "api|" } } ] }"#).1)
                .contains(&"api:codegen.schema")
        );
        assert_eq!(labels(&at(r#"{ "parallel": t|"#).1), vec!["true"]);
    }

    #[test]
    fn nothing_after_a_written_value() {
        assert!(at(r#"{ "hooks": [ { "reads": []| } ] }"#).1.is_empty());
        assert!(
            at(r#"{ "hooks": [ { "trigger": "pre-push" | } ] }"#)
                .1
                .is_empty()
        );
        assert_eq!(
            labels(&at(r#"{ "hooks": [ { "reads": ["version", "|"] } ] }"#).1),
            vec!["version"]
        );
    }

    #[test]
    fn keys_of_other_objects() {
        assert!(labels(&at(r#"{ "|"#).1).contains(&"hooks"));
        assert_eq!(
            labels(&at(r#"{ "variables": { "v": { "wr|" } } }"#).1),
            vec!["writers"]
        );
        assert_eq!(
            labels(&at(r#"{ "hooks": [ { "permissions": { "n|" } } ] }"#).1),
            vec!["network"]
        );
        assert!(labels(&at(r#"{ "hooks": [ { "steps": [ { "|" } ] } ] }"#).1).contains(&"id"));
    }
}
