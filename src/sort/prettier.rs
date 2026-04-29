//! Pipeline pass: `prettier` config sort. Mirrors upstream
//! `sort-package-json` v3.6.1 (`sortPrettierConfig`).
//!
//! - Top-level keys alpha-sorted, except `overrides` is forced last.
//! - Each `overrides[]` entry is alpha-sorted; its `options` sub-object is
//!   alpha-sorted too.

use serde_json::{Map, Value};

use super::helpers::{map_object_array, sort_object_alpha, sort_object_by_keys_iter};
use crate::configuration::Configuration;

/// Pipeline entry. Gated by `config.sort_nested`.
pub fn pass(mut object: Map<String, Value>, config: &Configuration) -> Map<String, Value> {
    if !config.sort_nested {
        return object;
    }
    if let Some(Value::Object(m)) = object.get_mut("prettier") {
        *m = sort_prettier_config(std::mem::take(m));
    }
    object
}

fn sort_prettier_config(map: Map<String, Value>) -> Map<String, Value> {
    // Build the desired order: every key except `overrides`, alpha-sorted,
    // then `overrides` appended.
    let mut other_keys: Vec<String> = map.keys().filter(|k| *k != "overrides").cloned().collect();
    other_keys.sort();
    let order = other_keys
        .iter()
        .map(String::as_str)
        .chain(std::iter::once("overrides"));

    let mut out = sort_object_by_keys_iter(map, order);

    if let Some(Value::Array(a)) = out.get_mut("overrides") {
        *a = map_object_array(std::mem::take(a), sort_override_entry);
    }

    out
}

fn sort_override_entry(entry: Map<String, Value>) -> Map<String, Value> {
    let mut out = sort_object_alpha(entry);
    if let Some(Value::Object(opts)) = out.get_mut("options") {
        *opts = sort_object_alpha(std::mem::take(opts));
    }
    out
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn run(input: Value) -> Value {
        let Value::Object(m) = input else {
            panic!("expected object")
        };
        Value::Object(pass(m, &Configuration::default()))
    }

    #[test]
    fn keys_alpha_overrides_last() {
        let out = run(json!({
            "prettier": {
                "tabWidth": 2,
                "overrides": [],
                "printWidth": 100,
                "semi": false
            }
        }));
        let keys: Vec<&str> = out["prettier"]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(keys, vec!["printWidth", "semi", "tabWidth", "overrides"]);
    }

    #[test]
    fn overrides_entry_alpha_with_options_alpha() {
        let out = run(json!({
            "prettier": {
                "overrides": [
                    {
                        "options": { "tabWidth": 4, "semi": true },
                        "files": "*.md",
                        "excludeFiles": "x"
                    }
                ]
            }
        }));
        let entry = &out["prettier"]["overrides"][0];
        let outer: Vec<&str> = entry
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        let opts: Vec<&str> = entry["options"]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(outer, vec!["excludeFiles", "files", "options"]);
        assert_eq!(opts, vec!["semi", "tabWidth"]);
    }

    #[test]
    fn no_overrides_is_fine() {
        let out = run(json!({
            "prettier": { "tabWidth": 2, "semi": false }
        }));
        let keys: Vec<&str> = out["prettier"]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(keys, vec!["semi", "tabWidth"]);
    }
}
