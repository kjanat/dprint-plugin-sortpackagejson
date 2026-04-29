use std::collections::HashSet;

use serde_json::{Map, Value};

/// Alphabetize an object's keys (one level only).
pub fn sort_object_alpha(map: Map<String, Value>) -> Map<String, Value> {
    let mut entries: Vec<(String, Value)> = map.into_iter().collect();
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    entries.into_iter().collect()
}

/// Reorder an object: listed keys first (in given order), unknowns alpha after.
/// Mirrors upstream `sortObjectKeys(obj, [keys...])`.
pub fn sort_object_by_keys(map: Map<String, Value>, order: &[&str]) -> Map<String, Value> {
    sort_object_by_keys_iter(map, order.iter().copied())
}

/// Like [`sort_object_by_keys`] but accepts any iterator of borrowed `&str`,
/// useful when the order is computed at runtime (`Vec<String>`).
pub fn sort_object_by_keys_iter<'a, I>(map: Map<String, Value>, order: I) -> Map<String, Value>
where
    I: IntoIterator<Item = &'a str>,
{
    let mut object = map;
    let mut out = Map::with_capacity(object.len());
    for key in order {
        if let Some(value) = object.shift_remove(key) {
            out.insert(key.to_string(), value);
        }
    }
    let mut rest: Vec<(String, Value)> = object.into_iter().collect();
    rest.sort_by(|a, b| a.0.cmp(&b.0));
    for (k, v) in rest {
        out.insert(k, v);
    }
    out
}

/// Apply a per-element transform to every object inside an array; non-object
/// elements pass through. Mirrors upstream `onArray((arr) => arr.map(over))`.
pub fn map_object_array<F>(array: Vec<Value>, mut over: F) -> Vec<Value>
where
    F: FnMut(Map<String, Value>) -> Map<String, Value>,
{
    array
        .into_iter()
        .map(|v| match v {
            Value::Object(m) => Value::Object(over(m)),
            other => other,
        })
        .collect()
}

/// Dedupe a string array, preserving first-seen order. Mirrors upstream `uniq`.
/// Non-string arrays pass through unchanged.
pub fn dedupe_string_array(array: Vec<Value>) -> Vec<Value> {
    let all_strings = array.iter().all(|v| matches!(v, Value::String(_)));
    if !all_strings {
        return array;
    }
    let mut seen: HashSet<String> = HashSet::with_capacity(array.len());
    let mut out: Vec<Value> = Vec::with_capacity(array.len());
    for item in array {
        if let Value::String(s) = item
            && seen.insert(s.clone())
        {
            out.push(Value::String(s));
        }
    }
    out
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
