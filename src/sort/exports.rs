//! Pipeline pass: `exports` recursive sort. Mirrors upstream
//! `sort-package-json` v3.6.1 (`sortExports`).
//!
//! Rules at every nesting level:
//!  - Keys starting with `.` are *paths*; keys not starting with `.` are
//!    *conditions* (`import`, `require`, `node`, `default`, ...).
//!  - Paths come first, in their original input order.
//!  - Conditions come second, in their original input order, except
//!    `default` is moved to the very end (it must come last per the Node
//!    resolution algorithm).
//!  - Recurse into every value that is itself an object.
//!
//! Note: upstream does NOT alpha-sort paths or conditions — it preserves
//! the user's intent because order is semantically meaningful in
//! conditional exports resolution.

use serde_json::{Map, Value};

use crate::configuration::Configuration;

/// Pipeline entry. Gated by `config.sort_nested`.
pub fn pass(mut object: Map<String, Value>, config: &Configuration) -> Map<String, Value> {
    if !config.sort_nested {
        return object;
    }
    if let Some(Value::Object(m)) = object.get_mut("exports") {
        *m = sort_exports(std::mem::take(m));
    }
    object
}

fn sort_exports(map: Map<String, Value>) -> Map<String, Value> {
    let mut paths: Vec<(String, Value)> = Vec::new();
    let mut conditions: Vec<(String, Value)> = Vec::new();
    let mut default_entry: Option<(String, Value)> = None;

    for (k, v) in map {
        let v = match v {
            Value::Object(child) => Value::Object(sort_exports(child)),
            other => other,
        };
        if k.starts_with('.') {
            paths.push((k, v));
        } else if k == "default" {
            default_entry = Some((k, v));
        } else {
            conditions.push((k, v));
        }
    }

    let mut out = Map::with_capacity(paths.len() + conditions.len() + 1);
    for (k, v) in paths {
        out.insert(k, v);
    }
    for (k, v) in conditions {
        out.insert(k, v);
    }
    if let Some((k, v)) = default_entry {
        out.insert(k, v);
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
    fn paths_before_conditions() {
        let out = run(json!({
            "exports": {
                "import": "./esm.js",
                ".": "./main.js",
                "require": "./cjs.js"
            }
        }));
        let keys: Vec<&str> = out["exports"]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(keys, vec![".", "import", "require"]);
    }

    #[test]
    fn default_condition_moved_to_end() {
        let out = run(json!({
            "exports": {
                "default": "./fallback.js",
                "import": "./esm.js",
                "require": "./cjs.js"
            }
        }));
        let keys: Vec<&str> = out["exports"]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(keys, vec!["import", "require", "default"]);
    }

    #[test]
    fn recursive_into_nested_paths() {
        let out = run(json!({
            "exports": {
                ".": {
                    "default": "./main.js",
                    "import": "./esm.js"
                },
                "./feature": {
                    "default": "./feat.js",
                    "node": "./node.js"
                }
            }
        }));
        let dot: Vec<&str> = out["exports"]["."]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        let feat: Vec<&str> = out["exports"]["./feature"]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(dot, vec!["import", "default"]);
        assert_eq!(feat, vec!["node", "default"]);
    }

    #[test]
    fn deep_default_at_every_level() {
        let out = run(json!({
            "exports": {
                "default": {
                    "default": {
                        "node": "./n.js",
                        "default": "./d.js"
                    },
                    "import": "./esm.js"
                },
                "import": "./top.js"
            }
        }));
        let outer: Vec<&str> = out["exports"]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        let inner1: Vec<&str> = out["exports"]["default"]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        let inner2: Vec<&str> = out["exports"]["default"]["default"]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(outer, vec!["import", "default"]);
        assert_eq!(inner1, vec!["import", "default"]);
        assert_eq!(inner2, vec!["node", "default"]);
    }

    #[test]
    fn paths_keep_input_order() {
        // Paths are NOT alpha-sorted; user's order is preserved.
        let out = run(json!({
            "exports": {
                "./z": "./z.js",
                "./a": "./a.js",
                ".": "./main.js"
            }
        }));
        let keys: Vec<&str> = out["exports"]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(keys, vec!["./z", "./a", "."]);
    }

    #[test]
    fn string_value_passes_through() {
        // Some packages use a bare string for `exports` (= `.` shorthand).
        let out = run(json!({ "exports": "./main.js" }));
        assert_eq!(out["exports"], json!("./main.js"));
    }
}
