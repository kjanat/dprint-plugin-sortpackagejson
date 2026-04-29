//! Regenerate `schema.json` from the `Configuration` struct via schemars.
//! Run via `cargo run --features schema --bin gen_schema > schema.json`.
//! Drift between code and schema is enforced by `tests/schema_in_sync.rs`.
//!
//! Output uses tab indentation to match the rest of the repo (dprint config
//! formats JSON with tabs); serde_json's default `to_string_pretty` uses 2
//! spaces, which would force a reformat after every regen.

use dprint_plugin_sortpackagejson::configuration::Configuration;
use serde::Serialize;
use serde_json::{
    Value,
    ser::{PrettyFormatter, Serializer},
};

fn main() {
    let schema = schemars::schema_for!(Configuration);
    let mut value = serde_json::to_value(&schema).expect("schema to value");

    if let Value::Object(map) = value {
        let repo_path = env!("CARGO_PKG_REPOSITORY").trim_start_matches("https://github.com/");
        let id = format!(
            "https://plugins.dprint.dev/{repo_path}/{}/schema.json",
            env!("CARGO_PKG_VERSION"),
        );

        let mut ordered = serde_json::Map::new();
        if let Some(v) = map.get("$schema").cloned() {
            ordered.insert("$schema".to_string(), v);
        }
        ordered.insert("$id".to_string(), Value::String(id));
        for (k, v) in map {
            if k != "$schema" {
                ordered.insert(k, v);
            }
        }
        value = Value::Object(ordered);
    }

    let mut buf = Vec::new();
    let formatter = PrettyFormatter::with_indent(b"\t");
    let mut ser = Serializer::with_formatter(&mut buf, formatter);
    value.serialize(&mut ser).expect("serialize schema");
    let out = String::from_utf8(buf).expect("schema is utf-8");
    println!("{out}");
}
