mod canonical;
mod dependencies;
mod helpers;
mod nested_alpha;
mod top_level;

use serde_json::{Map, Value};

use crate::configuration::Configuration;

/// Top-level entry point: sort a parsed `package.json` object.
///
/// Pass 1: reorder top-level keys by canonical / user `sort_order`.
/// Pass 2: apply per-field transforms (dependency-family alpha sort,
/// dedupe-and-sort string arrays, deep-sort `*Meta` maps; nested-section
/// sorts gated by `sort_nested`).
pub fn sort_package_json(input: Map<String, Value>, config: &Configuration) -> Map<String, Value> {
    let sorted = top_level::sort_top_level(input, config);
    let after_deps = apply_field_transforms(sorted, config);
    if config.sort_nested {
        nested_alpha::apply_nested_sorts(after_deps)
    } else {
        after_deps
    }
}

fn apply_field_transforms(
    mut object: Map<String, Value>,
    config: &Configuration,
) -> Map<String, Value> {
    use dependencies::{sort_by_package_ident_deep, sort_dependencies};
    use helpers::{dedupe_sort_string_array, sort_object_alpha_deep};

    if config.sort_dependencies {
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
    }

    object
}
