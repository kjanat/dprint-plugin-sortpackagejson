//! Pipeline pass: `pnpm` config sort. Mirrors upstream
//! `sort-package-json` (`sortPnpmConfig`).
//!
//! - Top level uses a fixed key order with the listed pnpm settings;
//!   unknowns alpha after.
//! - All nested objects deep-alpha sorted (`sortObjectBy(_, /* deep */ true)`).
//! - `pnpm.overrides` keys use a name-then-version-range comparator so that
//!   different version ranges of the same package are grouped.
//!
//! The name half uses ICU collation ([`super::collate`]) and the range half
//! compares minimum satisfying versions ([`super::semver_min`]), matching
//! upstream's `sortObjectBySemver` — so `foo@2` sorts before `foo@10` rather
//! than after it.

use std::cmp::Ordering;

use serde_json::{Map, Value};

use super::collate::compare_locale;
use super::helpers::{sort_object_alpha_deep, sort_object_by_keys};
use super::semver_min::min_version;
use crate::configuration::Configuration;

const PNPM_ORDER: &[&str] = &[
    "peerDependencyRules",
    "neverBuiltDependencies",
    "onlyBuiltDependencies",
    "onlyBuiltDependenciesFile",
    "allowedDeprecatedVersions",
    "allowNonAppliedPatches",
    "updateConfig",
    "auditConfig",
    "requiredScripts",
    "supportedArchitectures",
    "overrides",
    "patchedDependencies",
    "packageExtensions",
];

/// Pipeline entry. Gated by `config.sort_nested`.
pub fn pass(mut object: Map<String, Value>, config: &Configuration) -> Map<String, Value> {
    if !config.sort_nested {
        return object;
    }
    if let Some(Value::Object(m)) = object.get_mut("pnpm") {
        *m = sort_pnpm_config(std::mem::take(m));
    }
    object
}

fn sort_pnpm_config(map: Map<String, Value>) -> Map<String, Value> {
    // Step 1: deep-recurse into every value, alpha-sort nested objects.
    let recursed: Map<String, Value> = map
        .into_iter()
        .map(|(k, v)| {
            let v = match v {
                Value::Object(child) => Value::Object(sort_object_alpha_deep(child)),
                other => other,
            };
            (k, v)
        })
        .collect();

    // Step 2: top-level fixed order.
    let mut out = sort_object_by_keys(recursed, PNPM_ORDER);

    // Step 3: re-sort `overrides` by package-name then range.
    if let Some(Value::Object(ov)) = out.get_mut("overrides") {
        // The deep step in #1 already alpha-sorted nested objects inside
        // overrides values; we only need to reorder the top keys here.
        *ov = sort_overrides_by_ident_and_range(std::mem::take(ov));
    }

    out
}

fn sort_overrides_by_ident_and_range(map: Map<String, Value>) -> Map<String, Value> {
    let mut entries: Vec<(String, Value)> = map.into_iter().collect();
    entries.sort_by(|a, b| compare_specifier(&a.0, &b.0));
    entries.into_iter().collect()
}

/// Equivalent to upstream `sortObjectBySemver`'s comparator: package names
/// compare with ICU collation, and equal names tie-break on the minimum
/// version each range admits.
fn compare_specifier(a: &str, b: &str) -> Ordering {
    let (a_name, a_range) = parse_name_and_range(a);
    let (b_name, b_range) = parse_name_and_range(b);
    match compare_locale(a_name, b_name) {
        Ordering::Equal => match (a_range, b_range) {
            (None, None) => Ordering::Equal,
            (None, Some(_)) => Ordering::Less,
            (Some(_), None) => Ordering::Greater,
            (Some(ar), Some(br)) => compare_range(ar, br),
        },
        other => other,
    }
}

/// Compare two version ranges by the lowest version each admits, mirroring
/// upstream's `semverCompare(semverMinVersion(a), semverMinVersion(b))`.
///
/// Ranges node-semver cannot parse (`workspace:*`, `npm:foo@1`, ...) make it
/// throw; here they fall back to comparing the range text, so an unusual
/// override never breaks formatting.
fn compare_range(a: &str, b: &str) -> Ordering {
    match (min_version(a), min_version(b)) {
        (Some(left), Some(right)) => left.cmp(&right),
        _ => a.cmp(b),
    }
}

/// Mirrors upstream `parseNameAndVersionRange`: drop everything after `>`
/// (parent>child selectors), then split on the *last* `@`, treating a
/// leading `@` as part of a scoped package name.
fn parse_name_and_range(spec: &str) -> (&str, Option<&str>) {
    let head = spec.split('>').next().unwrap_or(spec);
    // Find every `@` index in `head`.
    let scope_offset = if head.starts_with('@') { 1 } else { 0 };
    let last_at = head[scope_offset..]
        .rfind('@')
        .map(|i| scope_offset + i)
        .filter(|&i| i > 0); // ignore the leading `@` of a scope-only name
    match last_at {
        Some(idx) => (&head[..idx], Some(&head[idx + 1..])),
        None => (head, None),
    }
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
    fn parse_name_and_range_handles_specifier_shapes() {
        assert_eq!(parse_name_and_range("foo"), ("foo", None));
        assert_eq!(parse_name_and_range("foo@1.0.0"), ("foo", Some("1.0.0")));
        assert_eq!(parse_name_and_range("foo@<1.0.0"), ("foo", Some("<1.0.0")));
        assert_eq!(parse_name_and_range("@scope/bar"), ("@scope/bar", None));
        assert_eq!(
            parse_name_and_range("@scope/bar@1"),
            ("@scope/bar", Some("1"))
        );
        // `>` denotes parent-child selector; we ignore the right side.
        assert_eq!(
            parse_name_and_range("foo>bar@1.0.0"),
            ("foo", None) // head=`foo`, no `@` in `foo`
        );
    }

    #[test]
    fn pnpm_top_level_fixed_then_alpha() {
        let out = run(json!({
            "pnpm": {
                "extra": "x",
                "overrides": {},
                "neverBuiltDependencies": [],
                "auditConfig": {},
                "peerDependencyRules": {}
            }
        }));
        let keys: Vec<&str> = out["pnpm"]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(
            keys,
            vec![
                "peerDependencyRules",
                "neverBuiltDependencies",
                "auditConfig",
                "overrides",
                "extra",
            ]
        );
    }

    #[test]
    fn pnpm_overrides_grouped_by_package_name() {
        let out = run(json!({
            "pnpm": {
                "overrides": {
                    "lodash@4.0.0": "4.17.0",
                    "@scope/bar": "1.0.0",
                    "lodash@3.0.0": "3.10.0",
                    "foo": "*"
                }
            }
        }));
        let keys: Vec<&str> = out["pnpm"]["overrides"]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        // Names: "@scope/bar" < "foo" < "lodash"; same-name lodash by range.
        assert_eq!(
            keys,
            vec!["@scope/bar", "foo", "lodash@3.0.0", "lodash@4.0.0",]
        );
    }

    #[test]
    fn nested_objects_inside_pnpm_are_alpha_sorted_deeply() {
        let out = run(json!({
            "pnpm": {
                "peerDependencyRules": {
                    "allowedVersions": { "z": "*", "a": "*" },
                    "ignoreMissing": ["c", "a", "b"]
                }
            }
        }));
        let inner: Vec<&str> = out["pnpm"]["peerDependencyRules"]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        let allowed: Vec<&str> = out["pnpm"]["peerDependencyRules"]["allowedVersions"]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(inner, vec!["allowedVersions", "ignoreMissing"]);
        assert_eq!(allowed, vec!["a", "z"]);
    }

    #[test]
    fn no_range_sorts_before_range_for_same_name() {
        let out = run(json!({
            "pnpm": {
                "overrides": {
                    "foo@1": "1.0.0",
                    "foo": "*"
                }
            }
        }));
        let keys: Vec<&str> = out["pnpm"]["overrides"]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(keys, vec!["foo", "foo@1"]);
    }

    #[test]
    fn overrides_ranges_compare_by_semver_not_text() {
        let out = run(json!({
            "pnpm": {
                "overrides": {
                    "foo@10": "a",
                    "foo@2": "b",
                    "foo@1": "c",
                    "bar": "d"
                }
            }
        }));
        let keys: Vec<&str> = out["pnpm"]["overrides"]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        // Lexicographically `foo@10` would precede `foo@2`.
        assert_eq!(keys, vec!["bar", "foo@1", "foo@2", "foo@10"]);
    }

    #[test]
    fn overrides_tolerate_non_semver_ranges() {
        let out = run(json!({
            "pnpm": {
                "overrides": {
                    "foo@workspace:*": "a",
                    "foo@1": "b"
                }
            }
        }));
        let keys: Vec<&str> = out["pnpm"]["overrides"]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        // Falls back to text order for the unparseable range; no panic.
        assert_eq!(keys, vec!["foo@1", "foo@workspace:*"]);
    }
}

