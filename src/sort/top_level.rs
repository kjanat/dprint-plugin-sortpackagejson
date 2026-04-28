use std::collections::HashSet;

use serde_json::{Map, Value};

use crate::configuration::{Configuration, UnknownKeyPolicy};

use super::canonical::CANONICAL_ORDER;

/// Reorder a `package.json` object's top-level keys.
///
/// Algorithm (mirroring upstream `sort-package-json`):
/// 1. Walk the user's `sort_order` (if non-empty) followed by the canonical
///    list. First occurrence wins.
/// 2. For each name in that combined order, if the input contains the key,
///    move it into the output.
/// 3. Append leftovers: public (no `_` prefix) then private (`_` prefix).
///    Each group is alphabetized when `unknown_keys = Alphabetical`,
///    otherwise input order is preserved within the group.
pub fn sort_top_level(input: Map<String, Value>, config: &Configuration) -> Map<String, Value> {
    let mut output = Map::with_capacity(input.len());
    let mut remaining = input;
    let mut seen: HashSet<&str> =
        HashSet::with_capacity(CANONICAL_ORDER.len() + config.sort_order.len());

    let primary = config
        .sort_order
        .iter()
        .map(String::as_str)
        .chain(CANONICAL_ORDER.iter().copied());

    for key in primary {
        if !seen.insert(key) {
            continue;
        }
        if let Some(value) = remaining.shift_remove(key) {
            output.insert(key.to_string(), value);
        }
    }

    let mut public_keys: Vec<(String, Value)> = Vec::new();
    let mut private_keys: Vec<(String, Value)> = Vec::new();
    for (k, v) in remaining {
        if k.starts_with('_') {
            private_keys.push((k, v));
        } else {
            public_keys.push((k, v));
        }
    }

    if matches!(config.unknown_keys, UnknownKeyPolicy::Alphabetical) {
        public_keys.sort_by(|a, b| a.0.cmp(&b.0));
        private_keys.sort_by(|a, b| a.0.cmp(&b.0));
    }

    for (k, v) in public_keys.into_iter().chain(private_keys) {
        output.insert(k, v);
    }

    output
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn cfg() -> Configuration {
        Configuration::default()
    }

    #[test]
    fn canonical_order_is_applied() {
        let input = json!({
            "version": "1.0.0",
            "name": "demo",
            "description": "x",
        });
        let object = input.as_object().unwrap().clone();
        let sorted = sort_top_level(object, &cfg());
        let keys: Vec<&str> = sorted.keys().map(String::as_str).collect();
        assert_eq!(keys, vec!["name", "version", "description"]);
    }

    #[test]
    fn unknown_public_keys_appended_alpha() {
        let input = json!({
            "zeta": 1,
            "name": "demo",
            "alpha": 2,
        });
        let object = input.as_object().unwrap().clone();
        let sorted = sort_top_level(object, &cfg());
        let keys: Vec<&str> = sorted.keys().map(String::as_str).collect();
        assert_eq!(keys, vec!["name", "alpha", "zeta"]);
    }

    #[test]
    fn private_keys_come_after_public_unknowns() {
        let input = json!({
            "_internal": 1,
            "name": "demo",
            "_aaa": 2,
            "zeta": 3,
        });
        let object = input.as_object().unwrap().clone();
        let sorted = sort_top_level(object, &cfg());
        let keys: Vec<&str> = sorted.keys().map(String::as_str).collect();
        assert_eq!(keys, vec!["name", "zeta", "_aaa", "_internal"]);
    }

    #[test]
    fn user_sort_order_takes_precedence() {
        let mut config = cfg();
        config.sort_order = vec!["description".to_string(), "name".to_string()];
        let input = json!({
            "name": "demo",
            "description": "x",
            "version": "1.0.0",
        });
        let object = input.as_object().unwrap().clone();
        let sorted = sort_top_level(object, &config);
        let keys: Vec<&str> = sorted.keys().map(String::as_str).collect();
        // user order honored; "version" still pulled from canonical fallback
        assert_eq!(keys, vec!["description", "name", "version"]);
    }

    #[test]
    fn preserve_keeps_unknown_input_order() {
        let mut config = cfg();
        config.unknown_keys = UnknownKeyPolicy::Preserve;
        let input = json!({
            "zeta": 1,
            "name": "demo",
            "alpha": 2,
        });
        let object = input.as_object().unwrap().clone();
        let sorted = sort_top_level(object, &config);
        let keys: Vec<&str> = sorted.keys().map(String::as_str).collect();
        assert_eq!(keys, vec!["name", "zeta", "alpha"]);
    }
}
