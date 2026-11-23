//! Parameters of the reusable hooks files:
//! the file declares `params`, used as `{{name}}`, that the hook importing it fills with `with`.
//!
//! ```json
//! { "params": { "config": { "default": ".eslintrc.json" }, "files": {} },
//!   "hooks": [ { "name": "eslint", "trigger": "pre-commit", "include": ["{{files}}"],
//!                "cmd": "eslint -c {{config}} {files}" } ] }
//! ```
//!
//! A string that is exactly `{{name}}` is replaced with the value as is
//! (a list fills an array); otherwise the value is inserted in the text.

use std::collections::BTreeMap;

use serde_json::{Map, Value};

use super::Param;

/// Replaces the `{{name}}` of `hooks` with the values of `with` (or the default values)
pub fn apply(
    hooks: &mut Value,
    params: &BTreeMap<String, Param>,
    with: Option<&BTreeMap<String, Value>>,
) -> Result<(), String> {
    let mut values = Map::new();
    for (name, value) in with.into_iter().flatten() {
        if !params.contains_key(name) {
            let known: Vec<&str> = params.keys().map(String::as_str).collect();
            return Err(format!(
                "unknown parameter `{name}` (parameters: {})",
                if known.is_empty() {
                    "none".to_string()
                } else {
                    known.join(", ")
                }
            ));
        }
        values.insert(name.clone(), value.clone());
    }
    for (name, param) in params {
        if values.contains_key(name) {
            continue;
        }
        match param.default.as_ref() {
            Some(default) => {
                values.insert(name.clone(), default.clone());
            }
            None => return Err(format!("parameter `{name}` is required, set it in `with`")),
        }
    }

    substitute(hooks, &values)
}

fn substitute(value: &mut Value, values: &Map<String, Value>) -> Result<(), String> {
    match value {
        Value::String(text) => {
            *value = replace(text, values)?;
        }
        Value::Array(items) => {
            let mut expanded = Vec::with_capacity(items.len());
            for mut item in std::mem::take(items) {
                substitute(&mut item, values)?;
                // `["{{files}}"]` with a list: the items fill the array
                match item {
                    Value::Array(inner) => expanded.extend(inner),
                    item => expanded.push(item),
                }
            }
            *items = expanded;
        }
        Value::Object(map) => {
            for (_, field) in map.iter_mut() {
                substitute(field, values)?;
            }
        }
        _ => {}
    }
    Ok(())
}

/// Replaces the `{{name}}` of a string
fn replace(text: &str, values: &Map<String, Value>) -> Result<Value, String> {
    // The whole string is a parameter: the type of the value is kept
    if let Some(name) = whole_placeholder(text) {
        return values
            .get(name)
            .cloned()
            .ok_or_else(|| format!("unknown parameter `{name}` in `{text}`"));
    }

    let mut result = String::new();
    let mut rest = text;
    while let Some(start) = rest.find("{{") {
        let Some(end) = rest[start..].find("}}") else {
            break;
        };
        let name = rest[start + 2..start + end].trim();
        let value = values
            .get(name)
            .ok_or_else(|| format!("unknown parameter `{name}` in `{text}`"))?;
        result.push_str(&rest[..start]);
        result.push_str(&match value {
            Value::String(s) => s.clone(),
            Value::Array(items) => items
                .iter()
                .map(|i| {
                    i.as_str()
                        .map(String::from)
                        .unwrap_or_else(|| i.to_string())
                })
                .collect::<Vec<_>>()
                .join(" "),
            other => other.to_string(),
        });
        rest = &rest[start + end + 2..];
    }
    result.push_str(rest);
    Ok(Value::String(result))
}

fn whole_placeholder(text: &str) -> Option<&str> {
    let inner = text.strip_prefix("{{")?.strip_suffix("}}")?.trim();
    (!inner.is_empty() && !inner.contains(['{', '}'])).then_some(inner)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn params(json: Value) -> BTreeMap<String, Param> {
        serde_json::from_value(json).unwrap()
    }

    #[test]
    fn replace_params() {
        let params = params(json!({ "config": { "default": ".eslintrc" }, "files": {} }));
        let with: BTreeMap<String, Value> =
            serde_json::from_value(json!({ "files": ["src/**/*.ts", "src/**/*.tsx"] })).unwrap();
        let mut hooks = json!([{
            "name": "eslint",
            "include": ["{{files}}", "*.json"],
            "cmd": "eslint -c {{config}} {files} -- {{ files }}"
        }]);

        apply(&mut hooks, &params, Some(&with)).unwrap();
        assert_eq!(
            hooks,
            json!([{
                "name": "eslint",
                "include": ["src/**/*.ts", "src/**/*.tsx", "*.json"],
                "cmd": "eslint -c .eslintrc {files} -- src/**/*.ts src/**/*.tsx"
            }])
        );
    }

    #[test]
    fn errors() {
        let declared = params(json!({ "files": {} }));
        let mut hooks = json!([{ "cmd": "{{files}}" }]);

        // missing required parameter
        let err = apply(&mut hooks.clone(), &declared, None).unwrap_err();
        assert!(err.contains("is required"), "{err}");

        // unknown parameter in `with`
        let with = serde_json::from_value(json!({ "file": "x" })).unwrap();
        let err = apply(&mut hooks.clone(), &declared, Some(&with)).unwrap_err();
        assert!(err.contains("unknown parameter `file`"), "{err}");

        // parameter not declared in the file
        let mut undeclared = json!([{ "cmd": "lint {{nope}}" }]);
        let with = serde_json::from_value(json!({ "files": "x" })).unwrap();
        let err = apply(&mut undeclared, &declared, Some(&with)).unwrap_err();
        assert!(err.contains("unknown parameter `nope`"), "{err}");

        // `{files}` (one brace) is left to the runner
        let with = serde_json::from_value(json!({ "files": "x" })).unwrap();
        apply(&mut hooks, &declared, Some(&with)).unwrap();
        let mut runtime = json!("cat {files}");
        substitute(&mut runtime, &Map::new()).unwrap();
        assert_eq!(runtime, json!("cat {files}"));
    }
}
