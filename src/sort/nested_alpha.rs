//! Per-key transforms applied to a sorted top-level object, gated by
//! `config.sort_nested`. Covers the "boring" cases: plain alpha objects,
//! fixed-order objects, arrays of fixed-order objects, git hooks, deep alpha,
//! and `uniq`-only string arrays. Scripts/exports/imports/eslint/prettier/
//! workspaces/pnpm have their own modules.
//!
//! Source-of-truth parity: `node_modules/sort-package-json/index.js` (v3.6.1).

use serde_json::{Map, Value};

use super::helpers::{
    dedupe_string_array, map_object_array, sort_object_alpha, sort_object_alpha_deep,
    sort_object_by_keys,
};
use crate::configuration::Configuration;

/// Pipeline pass: nested fixed-order/alpha sorts. Gated by `config.sort_nested`.
pub fn pass(object: Map<String, Value>, config: &Configuration) -> Map<String, Value> {
    if !config.sort_nested {
        return object;
    }
    apply_nested_sorts(object)
}

const URL_OBJECT_ORDER: &[&str] = &["type", "url"];
const PEOPLE_OBJECT_ORDER: &[&str] = &["name", "email", "url"];
const BUGS_ORDER: &[&str] = &["url", "email"];
const DIRECTORIES_ORDER: &[&str] = &["lib", "bin", "man", "doc", "example", "test"];
const BINARY_ORDER: &[&str] = &[
    "module_name",
    "module_path",
    "remote_path",
    "package_name",
    "host",
];
const VOLTA_ORDER: &[&str] = &["node", "npm", "yarn"];
const DEV_ENGINES_PM_ORDER: &[&str] = &["name", "version", "onFail"];
const VSCODE_BADGE_ORDER: &[&str] = &["description", "url", "href"];

/// Canonical git hook list (mirror of `git-hooks-list/index.json`, v3.x).
const GIT_HOOKS_ORDER: &[&str] = &[
    "applypatch-msg",
    "pre-applypatch",
    "post-applypatch",
    "pre-commit",
    "pre-merge-commit",
    "prepare-commit-msg",
    "commit-msg",
    "post-commit",
    "pre-rebase",
    "post-checkout",
    "post-merge",
    "pre-push",
    "pre-receive",
    "update",
    "proc-receive",
    "post-receive",
    "post-update",
    "reference-transaction",
    "push-to-checkout",
    "pre-auto-gc",
    "post-rewrite",
    "sendemail-validate",
    "fsmonitor-watchman",
    "p4-changelist",
    "p4-prepare-changelist",
    "p4-post-changelist",
    "p4-pre-submit",
    "post-index-change",
];

/// Plain one-level alpha sort, applied to the listed keys when present.
const ALPHA_KEYS: &[&str] = &[
    "bin",
    "contributes",
    "commitlint",
    "config",
    "nodemonConfig",
    "browserify",
    "babel",
    "xo",
    "npmpkgjsonlint",
    "npmPackageJsonLintConfig",
    "npmpackagejsonlint",
    "release",
    "remarkConfig",
    "ava",
    "jest",
    "jest-junit",
    "jest-stare",
    "mocha",
    "nyc",
    "c8",
    "tap",
    "engines",
    "engineStrict",
    "preferGlobal",
    "publishConfig",
    "galleryBanner",
];

/// Arrays where duplicates are removed but order is preserved (`uniq`).
const UNIQ_KEYS: &[&str] = &["categories", "keywords", "files", "activationEvents"];

pub fn apply_nested_sorts(mut object: Map<String, Value>) -> Map<String, Value> {
    for key in ALPHA_KEYS {
        if let Some(Value::Object(m)) = object.get_mut(*key) {
            *m = sort_object_alpha(std::mem::take(m));
        }
    }

    for key in UNIQ_KEYS {
        if let Some(Value::Array(a)) = object.get_mut(*key) {
            *a = dedupe_string_array(std::mem::take(a));
        }
    }

    apply_fixed_order(&mut object, "bugs", BUGS_ORDER);
    apply_fixed_order(&mut object, "repository", URL_OBJECT_ORDER);
    apply_fixed_order(&mut object, "funding", URL_OBJECT_ORDER);
    apply_fixed_order(&mut object, "license", URL_OBJECT_ORDER);
    apply_fixed_order(&mut object, "author", PEOPLE_OBJECT_ORDER);
    apply_fixed_order(&mut object, "directories", DIRECTORIES_ORDER);
    apply_fixed_order(&mut object, "binary", BINARY_ORDER);
    apply_fixed_order(&mut object, "volta", VOLTA_ORDER);

    apply_array_of_fixed(&mut object, "maintainers", PEOPLE_OBJECT_ORDER);
    apply_array_of_fixed(&mut object, "contributors", PEOPLE_OBJECT_ORDER);
    apply_array_of_fixed(&mut object, "badges", VSCODE_BADGE_ORDER);

    if let Some(Value::Object(m)) = object.get_mut("husky")
        && let Some(Value::Object(hooks)) = m.get_mut("hooks")
    {
        *hooks = sort_object_by_keys(std::mem::take(hooks), GIT_HOOKS_ORDER);
    }
    if let Some(Value::Object(m)) = object.get_mut("simple-git-hooks") {
        *m = sort_object_by_keys(std::mem::take(m), GIT_HOOKS_ORDER);
    }

    if let Some(Value::Object(m)) = object.get_mut("devEngines")
        && let Some(Value::Object(pm)) = m.get_mut("packageManager")
    {
        *pm = sort_object_by_keys(std::mem::take(pm), DEV_ENGINES_PM_ORDER);
    }

    if let Some(Value::Object(m)) = object.get_mut("oclif") {
        *m = sort_object_alpha_deep(std::mem::take(m));
    }

    object
}

fn apply_fixed_order(object: &mut Map<String, Value>, key: &str, order: &[&str]) {
    if let Some(Value::Object(m)) = object.get_mut(key) {
        *m = sort_object_by_keys(std::mem::take(m), order);
    }
}

fn apply_array_of_fixed(object: &mut Map<String, Value>, key: &str, order: &[&str]) {
    if let Some(Value::Array(a)) = object.get_mut(key) {
        *a = map_object_array(std::mem::take(a), |m| sort_object_by_keys(m, order));
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn run(input: Value) -> Value {
        let Value::Object(m) = input else { panic!() };
        Value::Object(apply_nested_sorts(m))
    }

    #[test]
    fn engines_alphabetized() {
        let out = run(json!({ "engines": { "node": ">=20", "bun": "*", "npm": "*" } }));
        let keys: Vec<&str> = out["engines"]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(keys, vec!["bun", "node", "npm"]);
    }

    #[test]
    fn repository_uses_type_url_order() {
        let out = run(json!({ "repository": { "url": "x", "type": "git" } }));
        let keys: Vec<&str> = out["repository"]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(keys, vec!["type", "url"]);
    }

    #[test]
    fn directories_uses_fixed_then_alpha_unknowns() {
        let out = run(json!({
            "directories": { "test": "t", "extra": "e", "lib": "l", "bin": "b" }
        }));
        let keys: Vec<&str> = out["directories"]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        // canonical order: lib, bin, man, doc, example, test → unknowns alpha after
        assert_eq!(keys, vec!["lib", "bin", "test", "extra"]);
    }

    #[test]
    fn keywords_dedupe_preserves_order() {
        let out = run(json!({ "keywords": ["b", "a", "b", "c", "a"] }));
        let arr: Vec<&str> = out["keywords"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert_eq!(arr, vec!["b", "a", "c"]);
    }

    #[test]
    fn maintainers_each_object_uses_people_order() {
        let out = run(json!({
            "maintainers": [
                { "url": "u", "name": "n", "email": "e" },
                { "email": "e2", "name": "n2" }
            ]
        }));
        let first: Vec<&str> = out["maintainers"][0]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        let second: Vec<&str> = out["maintainers"][1]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(first, vec!["name", "email", "url"]);
        assert_eq!(second, vec!["name", "email"]);
    }

    #[test]
    fn husky_hooks_use_git_hook_order() {
        let out = run(json!({
            "husky": { "hooks": { "post-commit": "x", "pre-commit": "y", "commit-msg": "z" } }
        }));
        let keys: Vec<&str> = out["husky"]["hooks"]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(keys, vec!["pre-commit", "commit-msg", "post-commit"]);
    }

    #[test]
    fn simple_git_hooks_use_git_hook_order() {
        let out = run(json!({
            "simple-git-hooks": { "post-commit": "x", "pre-push": "y", "pre-commit": "z" }
        }));
        let keys: Vec<&str> = out["simple-git-hooks"]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(keys, vec!["pre-commit", "post-commit", "pre-push"]);
    }

    #[test]
    fn dev_engines_package_manager_uses_fixed_order() {
        let out = run(json!({
            "devEngines": { "packageManager": { "version": "9", "name": "pnpm", "onFail": "warn" } }
        }));
        let keys: Vec<&str> = out["devEngines"]["packageManager"]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(keys, vec!["name", "version", "onFail"]);
    }

    #[test]
    fn oclif_is_deep_alpha() {
        let out = run(json!({
            "oclif": { "z": 1, "a": { "z": 2, "a": 3 } }
        }));
        let outer: Vec<&str> = out["oclif"]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        let inner: Vec<&str> = out["oclif"]["a"]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(outer, vec!["a", "z"]);
        assert_eq!(inner, vec!["a", "z"]);
    }

    #[test]
    fn non_object_field_passes_through() {
        // upstream guards via `onObject`/`onArray`; mismatched types are no-ops.
        let out = run(json!({ "engines": "not-an-object" }));
        assert_eq!(out["engines"], json!("not-an-object"));
    }
}
