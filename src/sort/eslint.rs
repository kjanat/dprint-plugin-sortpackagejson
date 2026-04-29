//! Pipeline pass: `eslintConfig` recursive sort. Mirrors upstream
//! `sort-package-json` v3.6.1 (`sortEslintConfig`).
//!
//! - Top level uses a fixed key order
//!   (`files, excludedFiles, env, parser, parserOptions, settings, plugins,
//!    extends, rules, overrides, globals, processor, noInlineConfig,
//!    reportUnusedDisableDirectives`).
//! - `env`, `globals`, `parserOptions`, `settings` get a one-level alpha sort.
//! - `rules` are sorted by *slash count* first (so root rules precede
//!   plugin-namespaced ones), then alphabetically.
//! - `overrides[]` recurses through this same pipeline (each override is a
//!   mini eslint config).

use std::cmp::Ordering;

use serde_json::{Map, Value};

use super::helpers::{map_object_array, sort_object_alpha, sort_object_by_keys};
use crate::configuration::Configuration;

const ESLINT_CONFIG_ORDER: &[&str] = &[
    "files",
    "excludedFiles",
    "env",
    "parser",
    "parserOptions",
    "settings",
    "plugins",
    "extends",
    "rules",
    "overrides",
    "globals",
    "processor",
    "noInlineConfig",
    "reportUnusedDisableDirectives",
];

/// Pipeline entry. Gated by `config.sort_nested`.
pub fn pass(mut object: Map<String, Value>, config: &Configuration) -> Map<String, Value> {
    if !config.sort_nested {
        return object;
    }
    if let Some(Value::Object(m)) = object.get_mut("eslintConfig") {
        *m = sort_eslint_config(std::mem::take(m));
    }
    object
}

fn sort_eslint_config(map: Map<String, Value>) -> Map<String, Value> {
    let mut out = sort_object_by_keys(map, ESLINT_CONFIG_ORDER);

    for key in &["env", "globals", "parserOptions", "settings"] {
        if let Some(Value::Object(m)) = out.get_mut(*key) {
            *m = sort_object_alpha(std::mem::take(m));
        }
    }

    if let Some(Value::Object(m)) = out.get_mut("rules") {
        *m = sort_rules(std::mem::take(m));
    }

    if let Some(Value::Array(a)) = out.get_mut("overrides") {
        *a = map_object_array(std::mem::take(a), sort_eslint_config);
    }

    out
}

/// Sort eslint rules by slash count first (root rules before plugin rules,
/// `plugin/rule` before `@scope/plugin/rule`), then alphabetically inside
/// each tier.
fn sort_rules(map: Map<String, Value>) -> Map<String, Value> {
    let mut entries: Vec<(String, Value)> = map.into_iter().collect();
    entries.sort_by(|a, b| {
        let a_slashes = a.0.bytes().filter(|&b| b == b'/').count();
        let b_slashes = b.0.bytes().filter(|&b| b == b'/').count();
        match a_slashes.cmp(&b_slashes) {
            Ordering::Equal => a.0.cmp(&b.0),
            other => other,
        }
    });
    entries.into_iter().collect()
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
    fn fixed_top_level_order() {
        let out = run(json!({
            "eslintConfig": {
                "rules": {},
                "env": {},
                "extends": "x",
                "parser": "p"
            }
        }));
        let keys: Vec<&str> = out["eslintConfig"]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(keys, vec!["env", "parser", "extends", "rules"]);
    }

    #[test]
    fn rules_sort_by_slash_count_then_alpha() {
        let out = run(json!({
            "eslintConfig": {
                "rules": {
                    "@scope/plugin/rule": "off",
                    "plugin/zeta": "off",
                    "no-undef": "error",
                    "plugin/alpha": "off",
                    "no-debugger": "error"
                }
            }
        }));
        let keys: Vec<&str> = out["eslintConfig"]["rules"]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(
            keys,
            vec![
                "no-debugger",
                "no-undef",
                "plugin/alpha",
                "plugin/zeta",
                "@scope/plugin/rule",
            ]
        );
    }

    #[test]
    fn env_globals_parser_options_settings_alpha() {
        let out = run(json!({
            "eslintConfig": {
                "env": { "node": true, "browser": true },
                "globals": { "Z": "readonly", "A": "writable" },
                "parserOptions": { "sourceType": "module", "ecmaVersion": 2024 },
                "settings": { "z": 1, "a": 2 }
            }
        }));
        for (key, expected) in &[
            ("env", vec!["browser", "node"]),
            ("globals", vec!["A", "Z"]),
            ("parserOptions", vec!["ecmaVersion", "sourceType"]),
            ("settings", vec!["a", "z"]),
        ] {
            let keys: Vec<&str> = out["eslintConfig"][key]
                .as_object()
                .unwrap()
                .keys()
                .map(String::as_str)
                .collect();
            assert_eq!(&keys, expected, "key {key}");
        }
    }

    #[test]
    fn overrides_recurse_into_each_entry() {
        let out = run(json!({
            "eslintConfig": {
                "overrides": [
                    {
                        "rules": { "no-undef": "error", "@a/b/c": "off" },
                        "files": ["*.ts"]
                    }
                ]
            }
        }));
        let entry = &out["eslintConfig"]["overrides"][0];
        let outer: Vec<&str> = entry
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        let rules: Vec<&str> = entry["rules"]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(outer, vec!["files", "rules"]);
        assert_eq!(rules, vec!["no-undef", "@a/b/c"]);
    }
}
