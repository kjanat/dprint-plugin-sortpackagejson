use serde_json::{Map, Value};

use super::collate::compare_locale;
use super::helpers::{
    dedupe_sort_string_array, sort_object_alpha, sort_object_alpha_deep, sort_object_with,
};
use crate::configuration::{Configuration, PackageManagerPolicy};

/// Pipeline pass: dependency-family alpha + uniq-and-sort + meta deep sort.
/// Gated by `config.sort_dependencies`.
pub fn pass(mut object: Map<String, Value>, config: &Configuration) -> Map<String, Value> {
    if !config.sort_dependencies {
        return object;
    }

    // Decided once from the whole document, before anything is moved.
    let like_npm = should_sort_dependencies_like_npm(&object, config.package_manager);

    // `get_mut` + `mem::take` keeps each key in its canonical position;
    // `shift_remove` + re-insert would push it to the end of the map.
    //
    // Only these five go through the package-manager-aware comparator —
    // upstream maps them to `sortDependencies` (index.js:554-561). Note
    // `resolutions` is deliberately absent: upstream sorts it with plain
    // `sortObject` (index.js:553).
    const DEP_OBJECT_KEYS: &[&str] = &[
        "dependencies",
        "devDependencies",
        "peerDependencies",
        "optionalDependencies",
        "overrides",
    ];
    for key in DEP_OBJECT_KEYS {
        if let Some(Value::Object(m)) = object.get_mut(*key) {
            *m = sort_dependencies(std::mem::take(m), like_npm);
        }
    }

    if let Some(Value::Object(m)) = object.get_mut("resolutions") {
        *m = sort_object_alpha(std::mem::take(m));
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
/// npm orders these with `localeCompare(_, 'en')` (see
/// [`super::collate`]); yarn and pnpm use a plain code-unit compare.
/// Mirrors upstream `sortDependencies` (index.js:157-171), including the
/// `len < 2` short-circuit.
pub fn sort_dependencies(map: Map<String, Value>, like_npm: bool) -> Map<String, Value> {
    if map.len() < 2 {
        return map;
    }
    if like_npm {
        sort_object_with(map, compare_locale)
    } else {
        sort_object_alpha(map)
    }
}

/// Decide whether to order dependency objects the way npm does.
///
/// Ports upstream `shouldSortDependenciesLikeNpm` (index.js:126-148) as far
/// as a sandboxed wasm plugin can: every in-document signal is honoured, but
/// the working-directory probe for `yarn.lock` / `.yarnrc.yml` /
/// `pnpm-lock.yaml` / `pnpm-workspace.yaml` is unreachable here. When no
/// in-document signal resolves it, this lands on npm — the same default
/// upstream reaches when it finds no lock file. `packageManager` in the
/// plugin config overrides the whole decision.
pub(super) fn should_sort_dependencies_like_npm(
    object: &Map<String, Value>,
    policy: PackageManagerPolicy,
) -> bool {
    match policy {
        PackageManagerPolicy::Npm => return true,
        PackageManagerPolicy::Other => return false,
        PackageManagerPolicy::Auto => {}
    }

    // https://github.com/nodejs/corepack
    if let Some(Value::String(pm)) = object.get("packageManager") {
        return pm.starts_with("npm@");
    }

    if let Some(name) = object
        .get("devEngines")
        .and_then(Value::as_object)
        .and_then(|m| m.get("packageManager"))
        .and_then(Value::as_object)
        .and_then(|m| m.get("name"))
        .and_then(Value::as_str)
    {
        return name == "npm";
    }

    if object.contains_key("pnpm") {
        return false;
    }

    if object
        .get("engines")
        .and_then(Value::as_object)
        .is_some_and(|m| m.contains_key("npm"))
    {
        return true;
    }

    // Upstream probes the filesystem here; we cannot. Default to npm.
    true
}

/// Sort `dependenciesMeta`-style objects whose keys may be `name` or
/// `name@version`. Mirrors upstream `sortObjectByIdent(_, /* deep */ true)`:
/// recurse first using the same package-ident comparator, then sort the
/// current level by the package-name portion of the key.
pub fn sort_by_package_ident_deep(map: Map<String, Value>) -> Map<String, Value> {
    let mut entries: Vec<(String, Value)> = map
        .into_iter()
        .map(|(k, v)| {
            let v = match v {
                Value::Object(m) => Value::Object(sort_by_package_ident_deep(m)),
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
    use serde_json::json;

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
        let sorted = sort_dependencies(map, false);
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

    #[test]
    fn ident_deep_recurses_with_package_ident_sort() {
        let mut nested = Map::new();
        nested.insert("foo@2".into(), Value::Bool(true));
        nested.insert("foo@1".into(), Value::Bool(false));

        let mut map = Map::new();
        map.insert("pkg".into(), Value::Object(nested));

        let sorted = sort_by_package_ident_deep(map);
        let Value::Object(nested_sorted) = sorted.get("pkg").expect("nested object") else {
            panic!("nested value should stay an object");
        };
        let keys: Vec<&str> = nested_sorted.keys().map(String::as_str).collect();
        assert_eq!(keys, vec!["foo@2", "foo@1"]);
    }

    #[test]
    fn npm_ordering_is_locale_aware() {
        let mut map = Map::new();
        map.insert("a-b".into(), Value::String("1".into()));
        map.insert("a_b".into(), Value::String("1".into()));
        let keys: Vec<String> = sort_dependencies(map, true).keys().cloned().collect();
        // ICU orders `_` before `-`; byte order is the reverse.
        assert_eq!(keys, vec!["a_b", "a-b"]);
    }

    #[test]
    fn non_npm_ordering_is_byte_wise() {
        let mut map = Map::new();
        map.insert("a-b".into(), Value::String("1".into()));
        map.insert("a_b".into(), Value::String("1".into()));
        let keys: Vec<String> = sort_dependencies(map, false).keys().cloned().collect();
        assert_eq!(keys, vec!["a-b", "a_b"]);
    }

    fn detect(value: Value) -> bool {
        let Value::Object(m) = value else {
            panic!("expected object")
        };
        should_sort_dependencies_like_npm(&m, PackageManagerPolicy::Auto)
    }

    #[test]
    fn detects_package_manager_from_document() {
        assert!(detect(json!({ "packageManager": "npm@10.0.0" })));
        assert!(!detect(json!({ "packageManager": "pnpm@9.0.0" })));
        assert!(detect(json!({ "devEngines": { "packageManager": { "name": "npm" } } })));
        assert!(!detect(json!({ "devEngines": { "packageManager": { "name": "yarn" } } })));
        assert!(!detect(json!({ "pnpm": {} })));
        assert!(detect(json!({ "engines": { "npm": ">=10" } })));
        // No in-document signal: upstream's fallback is npm.
        assert!(detect(json!({ "name": "x" })));
        // `packageManager` wins over a `pnpm` key.
        assert!(detect(json!({ "packageManager": "npm@10", "pnpm": {} })));
    }

    #[test]
    fn config_policy_overrides_detection() {
        let Value::Object(pnpm_doc) = json!({ "pnpm": {} }) else {
            unreachable!()
        };
        assert!(should_sort_dependencies_like_npm(
            &pnpm_doc,
            PackageManagerPolicy::Npm
        ));
        let Value::Object(npm_doc) = json!({ "packageManager": "npm@10" }) else {
            unreachable!()
        };
        assert!(!should_sort_dependencies_like_npm(
            &npm_doc,
            PackageManagerPolicy::Other
        ));
    }

    #[test]
    fn resolutions_stay_on_plain_compare() {
        // Upstream sorts `resolutions` with plain `sortObject`, not the
        // package-manager-aware comparator.
        let out = pass(
            json!({ "resolutions": { "a-b": "1", "a_b": "1" } })
                .as_object()
                .unwrap()
                .clone(),
            &Configuration::default(),
        );
        let keys: Vec<&str> = out["resolutions"]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(keys, vec!["a-b", "a_b"]);
    }
}

