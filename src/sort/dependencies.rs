use serde_json::{Map, Value};

use super::helpers::{dedupe_sort_string_array, sort_object_alpha, sort_object_alpha_deep};
use crate::configuration::Configuration;

/// Pipeline pass: dependency-family alpha + uniq-and-sort + meta deep sort.
/// Gated by `config.sort_dependencies`.
pub fn pass(mut object: Map<String, Value>, config: &Configuration) -> Map<String, Value> {
    if !config.sort_dependencies {
        return object;
    }

    // `get_mut` + `mem::take` keeps each key in its canonical position;
    // `shift_remove` + re-insert would push it to the end of the map.
    const DEP_OBJECT_KEYS: &[&str] = &[
        "dependencies",
        "devDependencies",
        "peerDependencies",
        "optionalDependencies",
        "overrides",
        "resolutions",
    ];
    for key in DEP_OBJECT_KEYS {
        if let Some(Value::Object(m)) = object.get_mut(*key) {
            *m = sort_dependencies(std::mem::take(m));
        }
    }

    const DEP_ARRAY_KEYS: &[&str] = &[
        "bundledDependencies",
        "bundleDependencies",
        "extensionPack",
        "extensionDependencies",
    ];
    for key in DEP_ARRAY_KEYS {
        if let Some(Value::Array(a)) = object.get_mut(*key) {
            *a = dedupe_sort_string_array(std::mem::take(a));
        }
    }

    if let Some(Value::Object(m)) = object.get_mut("peerDependenciesMeta") {
        *m = sort_object_alpha_deep(std::mem::take(m));
    }
    if let Some(Value::Object(m)) = object.get_mut("dependenciesMeta") {
        *m = sort_by_package_ident_deep(std::mem::take(m));
    }

    object
}

/// Sort a dependency-style object (`dependencies`, `devDependencies`, ...).
///
/// Currently uses simple lexicographic ordering. Upstream
/// `sort-package-json` switches between npm's locale-aware
/// `localeCompare(_, 'en')` and yarn/pnpm's plain string compare based on
/// the project's package manager. For all-ASCII lowercase keys (the
/// realistic majority) the two orderings agree, so v0.1.0 ships only the
/// plain compare; locale-aware ordering can be added later without
/// changing the public API.
pub fn sort_dependencies(map: Map<String, Value>) -> Map<String, Value> {
    if map.len() < 2 {
        return map;
    }
    sort_object_alpha(map)
}

/// Sort `dependenciesMeta`-style objects whose keys may be `name` or
/// `name@version`. Mirrors upstream `sortObjectByIdent(_, /* deep */ true)`:
/// recurse first to alpha-sort nested objects, then sort the current level
/// by the package-name portion of the key.
pub fn sort_by_package_ident_deep(map: Map<String, Value>) -> Map<String, Value> {
    let mut entries: Vec<(String, Value)> = map
        .into_iter()
        .map(|(k, v)| {
            let v = match v {
                Value::Object(m) => Value::Object(sort_object_alpha_deep(m)),
                other => other,
            };
            (k, v)
        })
        .collect();
    entries.sort_by(|a, b| package_name_part(&a.0).cmp(package_name_part(&b.0)));
    entries.into_iter().collect()
}

/// Extract the package-name part of an identifier like `foo`, `foo@1`,
/// `@scope/foo`, or `@scope/foo@1`. Skips a leading `@` so scoped names are
/// matched on the first `@` *after* the scope.
fn package_name_part(ident: &str) -> &str {
    let scope_offset = if ident.starts_with('@') { 1 } else { 0 };
    match ident[scope_offset..].find('@') {
        Some(idx) => &ident[..scope_offset + idx],
        None => ident,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn package_name_part_handles_scoped_and_versioned() {
        assert_eq!(package_name_part("foo"), "foo");
        assert_eq!(package_name_part("foo@1.0.0"), "foo");
        assert_eq!(package_name_part("@scope/foo"), "@scope/foo");
        assert_eq!(package_name_part("@scope/foo@1.0.0"), "@scope/foo");
    }

    #[test]
    fn sort_dependencies_alpha() {
        let mut map = Map::new();
        map.insert("zeta".into(), Value::String("1".into()));
        map.insert("alpha".into(), Value::String("2".into()));
        map.insert("beta".into(), Value::String("3".into()));
        let sorted = sort_dependencies(map);
        let keys: Vec<&str> = sorted.keys().map(String::as_str).collect();
        assert_eq!(keys, vec!["alpha", "beta", "zeta"]);
    }

    #[test]
    fn ident_deep_groups_by_package_name() {
        let mut map = Map::new();
        // In input order: foo@2, foo@1 — same package name, should keep input order.
        map.insert("foo@2".into(), Value::Object(Map::new()));
        map.insert("foo@1".into(), Value::Object(Map::new()));
        map.insert("@scope/bar".into(), Value::Object(Map::new()));
        let sorted = sort_by_package_ident_deep(map);
        let keys: Vec<&str> = sorted.keys().map(String::as_str).collect();
        // "@scope/bar" < "foo" lexicographically, then both `foo` entries in input order.
        assert_eq!(keys, vec!["@scope/bar", "foo@2", "foo@1"]);
    }
}
