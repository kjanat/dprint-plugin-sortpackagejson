//! Pipeline pass: `workspaces` (object form) sort. Mirrors upstream
//! `sort-package-json` v3.6.1 (`sortWorkspaces`).
//!
//! npm/yarn-classic accept `workspaces` as an *array*, which we leave
//! alone — order can be semantically meaningful for script execution.
//! The pnpm/bun *object form* gets:
//!  - `packages` then `catalog` then alpha-sorted unknowns at the top
//!  - `packages` array deduped + alpha-sorted
//!  - `catalog` (a dependency-style object) alpha-sorted

use serde_json::{Map, Value};

use super::dependencies::sort_dependencies;
use super::helpers::{dedupe_sort_string_array, sort_object_by_keys};
use crate::configuration::Configuration;

const WORKSPACES_ORDER: &[&str] = &["packages", "catalog"];

/// Pipeline entry. Gated by `config.sort_nested`.
pub fn pass(mut object: Map<String, Value>, config: &Configuration) -> Map<String, Value> {
    if !config.sort_nested {
        return object;
    }
    if let Some(Value::Object(m)) = object.get_mut("workspaces") {
        *m = sort_workspaces_object(std::mem::take(m));
    }
    object
}

fn sort_workspaces_object(map: Map<String, Value>) -> Map<String, Value> {
    let mut out = sort_object_by_keys(map, WORKSPACES_ORDER);

    if let Some(Value::Array(a)) = out.get_mut("packages") {
        *a = dedupe_sort_string_array(std::mem::take(a));
    }
    if let Some(Value::Object(m)) = out.get_mut("catalog") {
        *m = sort_dependencies(std::mem::take(m));
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
    fn array_form_passes_through() {
        // Array form is intentionally not reordered.
        let out = run(json!({ "workspaces": ["packages/b", "packages/a"] }));
        assert_eq!(out["workspaces"], json!(["packages/b", "packages/a"]));
    }

    #[test]
    fn object_form_packages_first_catalog_second() {
        let out = run(json!({
            "workspaces": {
                "extra": {},
                "catalog": { "lodash": "^4" },
                "packages": ["packages/b", "packages/a"]
            }
        }));
        let keys: Vec<&str> = out["workspaces"]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(keys, vec!["packages", "catalog", "extra"]);
    }

    #[test]
    fn packages_deduped_and_sorted() {
        let out = run(json!({
            "workspaces": {
                "packages": ["packages/b", "packages/a", "packages/b"]
            }
        }));
        assert_eq!(
            out["workspaces"]["packages"],
            json!(["packages/a", "packages/b"])
        );
    }

    #[test]
    fn catalog_alpha_sorted() {
        let out = run(json!({
            "workspaces": {
                "catalog": { "zeta": "*", "alpha": "*", "beta": "*" }
            }
        }));
        let keys: Vec<&str> = out["workspaces"]["catalog"]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(keys, vec!["alpha", "beta", "zeta"]);
    }
}
