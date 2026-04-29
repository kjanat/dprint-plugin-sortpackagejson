//! Pipeline pass: `scripts` and `betterScripts` sorting. Mirrors upstream
//! `sort-package-json` v3.6.1 (`sortScripts` + `sortScriptNames` +
//! `hasSequentialScript`).
//!
//! Algorithm summary:
//!  1. Walk all script names; strip a leading `pre`/`post` if the remainder
//!     is a default npm lifecycle (start/test/...) or already exists as a
//!     plain name. Mark such bases as `prefixable`.
//!  2. Unless any script invokes a sequential npm-run-all chain over a
//!     glob, recursively group by colon-namespace and alpha-sort each
//!     group; this puts `build` before `build:dev` before `build:prod`
//!     before `test`.
//!  3. Re-expand each prefixable base into the trio `[pre*, *, post*]`,
//!     then reorder the original object by the resulting name list.

use std::collections::{BTreeMap, HashSet};

use serde_json::{Map, Value};

use super::helpers::sort_object_by_keys_iter;
use crate::configuration::Configuration;

/// Default npm lifecycle scripts (the only ones where `pre`/`post` prefixes
/// are meaningful even without a corresponding base script in
/// `package.json`). See https://docs.npmjs.com/misc/scripts.
const DEFAULT_NPM_SCRIPTS: &[&str] = &[
    "install",
    "pack",
    "prepare",
    "publish",
    "restart",
    "shrinkwrap",
    "start",
    "stop",
    "test",
    "uninstall",
    "version",
];

/// Pipeline entry. Sorts `scripts` and `betterScripts` when
/// `config.sort_scripts` is on; otherwise pass-through.
pub fn pass(mut object: Map<String, Value>, config: &Configuration) -> Map<String, Value> {
    if !config.sort_scripts {
        return object;
    }
    let has_seq = has_sequential_script(&object);
    for key in &["scripts", "betterScripts"] {
        if let Some(Value::Object(m)) = object.get_mut(*key) {
            *m = sort_scripts(std::mem::take(m), has_seq);
        }
    }
    object
}

fn sort_scripts(map: Map<String, Value>, has_sequential: bool) -> Map<String, Value> {
    let names: Vec<String> = map.keys().cloned().collect();
    let names_set: HashSet<&str> = names.iter().map(String::as_str).collect();

    let mut prefixable: HashSet<String> = HashSet::new();
    let mut transformed: Vec<String> = Vec::with_capacity(names.len());
    for name in &names {
        let stripped_pre = name.strip_prefix("pre");
        let stripped_post = name.strip_prefix("post");
        let stripped = stripped_pre.or(stripped_post);
        match stripped {
            Some(rest) if DEFAULT_NPM_SCRIPTS.contains(&rest) || names_set.contains(rest) => {
                prefixable.insert(rest.to_string());
                transformed.push(rest.to_string());
            }
            _ => transformed.push(name.clone()),
        }
    }

    let ordered_bases = if has_sequential {
        transformed
    } else {
        sort_script_names(&transformed, "")
    };

    let mut name_list: Vec<String> = Vec::with_capacity(ordered_bases.len() * 3);
    for base in ordered_bases {
        if prefixable.contains(&base) {
            name_list.push(format!("pre{base}"));
            name_list.push(base.clone());
            name_list.push(format!("post{base}"));
        } else {
            name_list.push(base);
        }
    }

    sort_object_by_keys_iter(map, name_list.iter().map(String::as_str))
}

/// Recursive colon-namespace grouping. Equivalent to upstream
/// `sortScriptNames(keys, prefix = '')`.
fn sort_script_names(keys: &[String], prefix: &str) -> Vec<String> {
    let mut groups: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for key in keys {
        let rest = if prefix.is_empty() {
            key.as_str()
        } else {
            // upstream: `key.slice(prefix.length + 1)` — skip the prefix and
            // the colon between prefix and child.
            &key[prefix.len() + 1..]
        };
        let base = match rest.find(':') {
            Some(i) if i > 0 => {
                let take = if prefix.is_empty() {
                    i
                } else {
                    prefix.len() + 1 + i
                };
                key[..take].to_string()
            }
            _ => key.clone(),
        };
        groups.entry(base).or_default().push(key.clone());
    }

    let mut out: Vec<String> = Vec::new();
    for (group_key, mut children) in groups {
        let nested_prefix = format!("{group_key}:");
        let has_nested = children.len() > 1
            && children
                .iter()
                .any(|k| *k != group_key && k.starts_with(&nested_prefix));
        if has_nested {
            let mut direct: Vec<String> = children
                .iter()
                .filter(|k| **k == group_key || !k.starts_with(&nested_prefix))
                .cloned()
                .collect();
            let nested: Vec<String> = children
                .into_iter()
                .filter(|k| k.starts_with(&nested_prefix))
                .collect();
            direct.sort();
            out.extend(direct);
            out.extend(sort_script_names(&nested, &group_key));
        } else {
            children.sort();
            out.extend(children);
        }
    }
    out
}

/// True iff the package depends on `npm-run-all`/`npm-run-all2` *and* any
/// script value contains a `*` glob driven through a sequential runner. In
/// that case we can't safely reorder scripts (the `*` is a positional
/// match against ordering of declarations).
fn has_sequential_script(package_json: &Map<String, Value>) -> bool {
    if !has_dev_dependency("npm-run-all", package_json)
        && !has_dev_dependency("npm-run-all2", package_json)
    {
        return false;
    }
    for field in &["scripts", "betterScripts"] {
        if let Some(Value::Object(m)) = package_json.get(*field) {
            for v in m.values() {
                if let Value::String(s) = v
                    && s.contains('*')
                    && matches_run_s_pattern(s)
                {
                    return true;
                }
            }
        }
    }
    false
}

fn has_dev_dependency(dep: &str, package_json: &Map<String, Value>) -> bool {
    matches!(
        package_json.get("devDependencies"),
        Some(Value::Object(m)) if m.contains_key(dep)
    )
}

/// Hand-rolled equivalent of upstream's `runSRegExp`:
///
/// ```text
/// (?<=^|[\s&;<>|(])
///     (?:run-s|npm-run-all2? .*(?:--sequential|--serial|-s))
/// (?=$|[\s&;<>|)])
/// ```
///
/// Either a bare `run-s` invocation, or `npm-run-all`/`npm-run-all2` with a
/// sequential flag somewhere in its arg list. Word-boundary checks use the
/// same shell-token separators as upstream.
fn matches_run_s_pattern(s: &str) -> bool {
    if has_word_with_boundaries(s, "run-s") {
        return true;
    }
    for prefix in &["npm-run-all2", "npm-run-all"] {
        let mut start = 0;
        while let Some(off) = s[start..].find(prefix) {
            let abs = start + off;
            let after = abs + prefix.len();
            if !left_separator_ok(s, abs) || after >= s.len() || s.as_bytes()[after] != b' ' {
                start = abs + 1;
                continue;
            }
            let rest = &s[after + 1..];
            if contains_sequential_flag(rest) {
                return true;
            }
            start = abs + 1;
        }
    }
    false
}

fn contains_sequential_flag(s: &str) -> bool {
    for flag in &["--sequential", "--serial", "-s"] {
        let mut start = 0;
        while let Some(off) = s[start..].find(flag) {
            let abs = start + off;
            let after = abs + flag.len();
            if after >= s.len() || is_right_separator_byte(s.as_bytes()[after]) {
                return true;
            }
            start = abs + 1;
        }
    }
    false
}

fn has_word_with_boundaries(s: &str, needle: &str) -> bool {
    let mut start = 0;
    while let Some(off) = s[start..].find(needle) {
        let abs = start + off;
        let after = abs + needle.len();
        let r_ok = after >= s.len() || is_right_separator_byte(s.as_bytes()[after]);
        if left_separator_ok(s, abs) && r_ok {
            return true;
        }
        start = abs + 1;
    }
    false
}

fn left_separator_ok(s: &str, idx: usize) -> bool {
    if idx == 0 {
        return true;
    }
    // ASCII separators only — non-ASCII trailing bytes can never alias these.
    matches!(
        s.as_bytes()[idx - 1],
        b' ' | b'\t' | b'\n' | b'\r' | b'&' | b';' | b'<' | b'>' | b'|' | b'('
    )
}

fn is_right_separator_byte(b: u8) -> bool {
    matches!(
        b,
        b' ' | b'\t' | b'\n' | b'\r' | b'&' | b';' | b'<' | b'>' | b'|' | b')'
    )
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
    fn pre_main_post_grouped() {
        let out = run(json!({
            "scripts": {
                "postbuild": "p", "build": "b", "prebuild": "pre"
            }
        }));
        let keys: Vec<&str> = out["scripts"]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(keys, vec!["prebuild", "build", "postbuild"]);
    }

    #[test]
    fn pre_post_for_default_lifecycle() {
        // `pretest` is recognized even though `test` is absent from scripts —
        // `test` is a default npm lifecycle.
        let out = run(json!({
            "scripts": {
                "pretest": "p", "lint": "l"
            }
        }));
        let keys: Vec<&str> = out["scripts"]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(keys, vec!["lint", "pretest"]);
    }

    #[test]
    fn colon_namespace_groups_with_parents_first() {
        let out = run(json!({
            "scripts": {
                "test": "t",
                "build:prod": "p",
                "build": "b",
                "build:dev": "d"
            }
        }));
        let keys: Vec<&str> = out["scripts"]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(keys, vec!["build", "build:dev", "build:prod", "test"]);
    }

    #[test]
    fn deep_colon_namespace() {
        let out = run(json!({
            "scripts": {
                "build:dev:web": "1",
                "build:dev:api": "2",
                "build:dev": "3",
                "build": "4"
            }
        }));
        let keys: Vec<&str> = out["scripts"]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(
            keys,
            vec!["build", "build:dev", "build:dev:api", "build:dev:web"]
        );
    }

    #[test]
    fn sequential_run_s_disables_namespace_grouping() {
        // npm-run-all + a `*` script => keep input order for bases.
        let out = run(json!({
            "devDependencies": { "npm-run-all": "*" },
            "scripts": {
                "test": "t",
                "build:prod": "p",
                "build:dev": "d",
                "all": "run-s build:*"
            }
        }));
        let keys: Vec<&str> = out["scripts"]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        // Input order preserved (no recursive group-sort).
        assert_eq!(keys, vec!["test", "build:prod", "build:dev", "all"]);
    }

    #[test]
    fn run_s_without_glob_does_not_disable_grouping() {
        // `*` is required for sequential detection.
        let out = run(json!({
            "devDependencies": { "npm-run-all": "*" },
            "scripts": {
                "test": "t",
                "build:dev": "d",
                "build:prod": "p",
                "all": "run-s build:dev build:prod"
            }
        }));
        let keys: Vec<&str> = out["scripts"]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(keys, vec!["all", "build:dev", "build:prod", "test"]);
    }

    #[test]
    fn npm_run_all_with_sequential_flag() {
        let out = run(json!({
            "devDependencies": { "npm-run-all2": "*" },
            "scripts": {
                "build:prod": "p",
                "build:dev": "d",
                "all": "npm-run-all2 --sequential build:*"
            }
        }));
        let keys: Vec<&str> = out["scripts"]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(keys, vec!["build:prod", "build:dev", "all"]);
    }

    #[test]
    fn run_s_word_boundary() {
        assert!(matches_run_s_pattern("run-s build"));
        assert!(matches_run_s_pattern("(run-s a)"));
        assert!(matches_run_s_pattern("run-s"));
        assert!(matches_run_s_pattern("foo && run-s a"));
        // Not a word boundary on left.
        assert!(!matches_run_s_pattern("xrun-s a"));
        // Not a word boundary on right.
        assert!(!matches_run_s_pattern("run-stuff a"));
    }

    #[test]
    fn npm_run_all_pattern_requires_flag() {
        assert!(!matches_run_s_pattern("npm-run-all build"));
        assert!(matches_run_s_pattern("npm-run-all build --sequential"));
        assert!(matches_run_s_pattern("npm-run-all -s build"));
        assert!(matches_run_s_pattern("npm-run-all2 --serial a b"));
        // Flag must be a separated token.
        assert!(!matches_run_s_pattern("npm-run-all build --sequentialish"));
    }

    #[test]
    fn betterscripts_also_sorted() {
        let out = run(json!({
            "betterScripts": {
                "test": { "command": "t" },
                "build": { "command": "b" }
            }
        }));
        let keys: Vec<&str> = out["betterScripts"]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(keys, vec!["build", "test"]);
    }

    #[test]
    fn missing_lifecycle_base_keeps_prefixed_name() {
        // Stripping `pre` from `prelude` yields `lude`, which is neither a
        // default lifecycle nor an existing script. Keep `prelude` as-is.
        let out = run(json!({
            "scripts": { "prelude": "p", "build": "b" }
        }));
        let keys: Vec<&str> = out["scripts"]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(keys, vec!["build", "prelude"]);
    }
}
