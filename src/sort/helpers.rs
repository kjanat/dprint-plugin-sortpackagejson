use std::collections::HashSet;

use serde_json::{Map, Value};

/// Alphabetize an object's keys (one level only).
pub fn sort_object_alpha(map: Map<String, Value>) -> Map<String, Value> {
    let mut entries: Vec<(String, Value)> = map.into_iter().collect();
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    entries.into_iter().collect()
}

/// Recursively alphabetize all nested objects (bottom-up), then this level.
/// Mirrors upstream `sortObjectBy(undefined, /* deep */ true)`.
pub fn sort_object_alpha_deep(map: Map<String, Value>) -> Map<String, Value> {
    let mut entries: Vec<(String, Value)> = map
        .into_iter()
        .map(|(k, v)| (k, recurse_alpha(v)))
        .collect();
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    entries.into_iter().collect()
}

fn recurse_alpha(value: Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(sort_object_alpha_deep(map)),
        other => other,
    }
}

/// Dedupe a string array and sort alphabetically.
///
/// Mirrors upstream `uniqAndSortArray` composed with `onStringArray`: if
/// any element is non-string, the array is passed through unchanged.
pub fn dedupe_sort_string_array(array: Vec<Value>) -> Vec<Value> {
    let all_strings = array.iter().all(|v| matches!(v, Value::String(_)));
    if !all_strings {
        return array;
    }
    let mut seen: HashSet<String> = HashSet::with_capacity(array.len());
    let mut deduped: Vec<String> = Vec::with_capacity(array.len());
    for item in array {
        if let Value::String(s) = item
            && seen.insert(s.clone())
        {
            deduped.push(s);
        }
    }
    deduped.sort();
    deduped.into_iter().map(Value::String).collect()
}
