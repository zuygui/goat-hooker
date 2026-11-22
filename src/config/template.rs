//! Paramètres des fichiers de hooks réutilisables :
//! le fichier déclare des `params`, utilisés en `{{nom}}`, que le hook qui l'importe remplit avec `with`.
//!
//! ```json
//! { "params": { "config": { "default": ".eslintrc.json" }, "files": {} },
//!   "hooks": [ { "name": "eslint", "trigger": "pre-commit", "include": ["{{files}}"],
//!                "cmd": "eslint -c {{config}} {files}" } ] }
//! ```
//!
//! Une chaîne qui est exactement `{{nom}}` est remplacée par la valeur telle quelle
//! (une liste remplit un tableau) ; sinon la valeur est insérée dans le texte.

use std::collections::BTreeMap;

use serde_json::{Map, Value};

use super::Param;

/// Remplace les `{{nom}}` de `hooks` par les valeurs de `with` (ou les valeurs par défaut)
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
                // `["{{files}}"]` avec une liste : les éléments remplissent le tableau
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

/// Remplace les `{{nom}}` d'une chaîne
fn replace(text: &str, values: &Map<String, Value>) -> Result<Value, String> {
    // La chaîne entière est un paramètre : on garde le type de la valeur
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

        // paramètre requis manquant
        let err = apply(&mut hooks.clone(), &declared, None).unwrap_err();
        assert!(err.contains("is required"), "{err}");

        // paramètre inconnu dans `with`
        let with = serde_json::from_value(json!({ "file": "x" })).unwrap();
        let err = apply(&mut hooks.clone(), &declared, Some(&with)).unwrap_err();
        assert!(err.contains("unknown parameter `file`"), "{err}");

        // paramètre non déclaré dans le fichier
        let mut undeclared = json!([{ "cmd": "lint {{nope}}" }]);
        let with = serde_json::from_value(json!({ "files": "x" })).unwrap();
        let err = apply(&mut undeclared, &declared, Some(&with)).unwrap_err();
        assert!(err.contains("unknown parameter `nope`"), "{err}");

        // `{files}` (une accolade) est laissé au runner
        let with = serde_json::from_value(json!({ "files": "x" })).unwrap();
        apply(&mut hooks, &declared, Some(&with)).unwrap();
        let mut runtime = json!("cat {files}");
        substitute(&mut runtime, &Map::new()).unwrap();
        assert_eq!(runtime, json!("cat {files}"));
    }
}
