//! `wireit` — Google's task runner (<https://github.com/google/wireit>).
//!
//! Added upstream in `sort-package-json` 4.0.0; it is the only behavioural
//! sorting change between 3.6.1 and 4.0.0. The key sits directly after
//! `betterScripts` in the canonical order.
//!
//! Shape (mirrors upstream `sortWireit` / `sortWireitScript`):
//!
//! - script names alphabetised
//! - each script: `command`, `dependencies`, `files`, `output`, then the rest
//! - `dependencies[]` objects: `script`, `cascade`, then the rest (plain
//!   string dependencies pass through untouched)
//! - `env`: keys alphabetised, each object value ordered `external`, `default`
//! - `service`: `readyWhen` first, and `readyWhen`'s own keys alphabetised

use serde_json::{Map, Value};

use super::helpers::{map_object_array, sort_object_alpha, sort_object_by_keys};
use crate::configuration::Configuration;

const SCRIPT_PROPERTIES: &[&str] = &["command", "dependencies", "files", "output"];

/// Pipeline pass: normalise the `wireit` task graph. Gated by
/// `config.sort_nested`.
pub fn pass(mut object: Map<String, Value>, config: &Configuration) -> Map<String, Value> {
    if !config.sort_nested {
        return object;
    }
    // `get_mut` + `mem::take` keeps `wireit` in its canonical position.
    if let Some(Value::Object(m)) = object.get_mut("wireit") {
        *m = sort_wireit(std::mem::take(m));
    }
    object
}

fn sort_wireit(map: Map<String, Value>) -> Map<String, Value> {
    let sorted: Map<String, Value> = map
        .into_iter()
        .map(|(name, script)| {
            let script = match script {
                Value::Object(m) => Value::Object(sort_wireit_script(m)),
                other => other,
            };
            (name, script)
        })
        .collect();
    sort_object_alpha(sorted)
}

fn sort_wireit_script(map: Map<String, Value>) -> Map<String, Value> {
    let mut out = sort_object_by_keys(map, SCRIPT_PROPERTIES);

    // `dependencies` may mix bare script names (strings) with objects.
    if let Some(Value::Array(a)) = out.get_mut("dependencies") {
        *a = map_object_array(std::mem::take(a), |m| {
            sort_object_by_keys(m, &["script", "cascade"])
        });
    }

    if let Some(Value::Object(m)) = out.get_mut("env") {
        *m = sort_env(std::mem::take(m));
    }

    if let Some(Value::Object(m)) = out.get_mut("service") {
        *m = sort_service(std::mem::take(m));
    }

    out
}

fn sort_env(map: Map<String, Value>) -> Map<String, Value> {
    let mapped: Map<String, Value> = map
        .into_iter()
        .map(|(key, value)| {
            let value = match value {
                Value::Object(m) => Value::Object(sort_object_by_keys(m, &["external", "default"])),
                other => other,
            };
            (key, value)
        })
        .collect();
    sort_object_alpha(mapped)
}

fn sort_service(map: Map<String, Value>) -> Map<String, Value> {
    let mut out = sort_object_by_keys(map, &["readyWhen"]);
    if let Some(Value::Object(m)) = out.get_mut("readyWhen") {
        *m = sort_object_alpha(std::mem::take(m));
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

    fn keys(value: &Value) -> Vec<&str> {
        value
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect()
    }

    #[test]
    fn script_names_are_alphabetised() {
        let out = run(json!({
            "wireit": { "test": {}, "build": {}, "attend": {} }
        }));
        assert_eq!(keys(&out["wireit"]), vec!["attend", "build", "test"]);
    }

    #[test]
    fn script_properties_take_fixed_order_then_alpha() {
        let out = run(json!({
            "wireit": {
                "build": {
                    "zulu": 1,
                    "output": ["dist"],
                    "alpha": 2,
                    "command": "tsc",
                    "files": ["src"],
                    "dependencies": []
                }
            }
        }));
        assert_eq!(
            keys(&out["wireit"]["build"]),
            vec!["command", "dependencies", "files", "output", "alpha", "zulu"]
        );
    }

    #[test]
    fn dependency_objects_ordered_string_deps_untouched() {
        let out = run(json!({
            "wireit": {
                "build": {
                    "dependencies": [
                        "plain-name",
                        { "cascade": false, "extra": 1, "script": "compile" }
                    ]
                }
            }
        }));
        let deps = out["wireit"]["build"]["dependencies"].as_array().unwrap();
        assert_eq!(deps[0], json!("plain-name"));
        assert_eq!(keys(&deps[1]), vec!["script", "cascade", "extra"]);
    }

    #[test]
    fn env_keys_alphabetised_and_values_ordered() {
        let out = run(json!({
            "wireit": {
                "build": {
                    "env": {
                        "ZED": { "default": "x", "external": true },
                        "ALPHA": "literal"
                    }
                }
            }
        }));
        let env = &out["wireit"]["build"]["env"];
        assert_eq!(keys(env), vec!["ALPHA", "ZED"]);
        assert_eq!(keys(&env["ZED"]), vec!["external", "default"]);
        assert_eq!(env["ALPHA"], json!("literal"));
    }

    #[test]
    fn service_ready_when_first_and_sorted() {
        let out = run(json!({
            "wireit": {
                "serve": {
                    "service": {
                        "other": 1,
                        "readyWhen": { "zebra": 1, "lineMatches": "ready" }
                    }
                }
            }
        }));
        let service = &out["wireit"]["serve"]["service"];
        assert_eq!(keys(service), vec!["readyWhen", "other"]);
        assert_eq!(keys(&service["readyWhen"]), vec!["lineMatches", "zebra"]);
    }

    #[test]
    fn non_object_service_passes_through() {
        let out = run(json!({ "wireit": { "serve": { "service": true } } }));
        assert_eq!(out["wireit"]["serve"]["service"], json!(true));
    }

    #[test]
    fn respects_sort_nested_disabled() {
        let Value::Object(m) = json!({ "wireit": { "b": {}, "a": {} } }) else {
            unreachable!()
        };
        let config = Configuration {
            sort_nested: false,
            ..Configuration::default()
        };
        let out = Value::Object(pass(m, &config));
        assert_eq!(keys(&out["wireit"]), vec!["b", "a"]);
    }
}
